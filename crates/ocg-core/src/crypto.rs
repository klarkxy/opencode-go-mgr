//! Simple obfuscation for API keys.
//!
//! This is intentionally lightweight: keys are not stored in plain text on disk,
//! but the scheme is NOT a substitute for a real KMS or AES-GCM. If stronger
//! security is needed later, replace this module with `ring`/`aes-gcm`.
//!
//! Two cipher implementations are provided:
//! - `MachineBoundCipher`: derives a key from Windows environment variables
//!   (USERNAME, COMPUTERNAME, APPDATA) for backward compatibility with the
//!   original GUI app.
//! - `StaticKeyCipher`: derives a key from an arbitrary user-supplied secret,
//!   suitable for headless / cross-platform / Docker deployments.

use anyhow::Context;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::Path;

const NONCE_LEN: usize = 16;

/// Trait for pluggable key obfuscation.
pub trait KeyCipher: Send + Sync {
    fn encrypt(&self, plaintext: &str) -> anyhow::Result<String>;
    fn decrypt(&self, ciphertext: &str) -> anyhow::Result<String>;
}

fn derive_key(seed: &[u8], len: usize) -> Vec<u8> {
    // Simple key-expansion using a basic hash-like mixer. Good enough to stop
    // casual plaintext inspection; explicitly not cryptographically secure.
    let mut key = Vec::with_capacity(len);
    let mut state: u64 = 0xcbf29ce484222325;
    let mut idx = 0;
    while key.len() < len {
        let b = seed.get(idx % seed.len().max(1)).copied().unwrap_or(0);
        state ^= b as u64;
        state = state.wrapping_mul(0x100000001b3);
        key.push((state >> 24) as u8);
        key.push((state >> 16) as u8);
        key.push((state >> 8) as u8);
        key.push(state as u8);
        idx += 1;
    }
    key.truncate(len);
    key
}

fn xor_encrypt(plaintext: &str, key_seed: &[u8]) -> anyhow::Result<String> {
    if plaintext.is_empty() {
        return Ok(String::new());
    }
    let bytes = plaintext.as_bytes();
    let nonce: Vec<u8> = uuid::Uuid::new_v4().as_bytes().to_vec();
    let key = derive_key(key_seed, bytes.len());
    let mut cipher = Vec::with_capacity(NONCE_LEN + bytes.len());
    cipher.extend_from_slice(&nonce);
    for (i, b) in bytes.iter().enumerate() {
        cipher.push(b ^ key[i] ^ nonce[i % NONCE_LEN]);
    }
    Ok(STANDARD.encode(&cipher))
}

fn xor_decrypt(ciphertext: &str, key_seed: &[u8]) -> anyhow::Result<String> {
    if ciphertext.is_empty() {
        return Ok(String::new());
    }
    let cipher = STANDARD.decode(ciphertext)?;
    if cipher.len() < NONCE_LEN {
        anyhow::bail!("invalid cipher text");
    }
    let (nonce, body) = cipher.split_at(NONCE_LEN);
    let key = derive_key(key_seed, body.len());
    let mut plain = Vec::with_capacity(body.len());
    for (i, b) in body.iter().enumerate() {
        plain.push(b ^ key[i] ^ nonce[i % NONCE_LEN]);
    }
    String::from_utf8(plain).map_err(|e| anyhow::anyhow!(e))
}

/// Original Windows machine-bound cipher.
#[derive(Debug, Clone, Default)]
pub struct MachineBoundCipher;

impl MachineBoundCipher {
    pub fn new() -> Self {
        Self
    }

    fn seed(&self) -> Vec<u8> {
        let mut parts = Vec::new();
        if let Ok(user) = env::var("USERNAME") {
            parts.push(user);
        }
        if let Ok(computer) = env::var("COMPUTERNAME") {
            parts.push(computer);
        }
        if let Ok(appdata) = env::var("APPDATA") {
            parts.push(appdata);
        }
        parts.join("|").into_bytes()
    }
}

impl KeyCipher for MachineBoundCipher {
    fn encrypt(&self, plaintext: &str) -> anyhow::Result<String> {
        xor_encrypt(plaintext, &self.seed())
    }

    fn decrypt(&self, ciphertext: &str) -> anyhow::Result<String> {
        xor_decrypt(ciphertext, &self.seed())
    }
}

/// Cross-platform cipher based on a user-provided secret.
#[derive(Debug, Clone)]
pub struct StaticKeyCipher {
    seed: Vec<u8>,
}

impl StaticKeyCipher {
    pub fn new(secret: &str) -> Self {
        Self {
            seed: secret.as_bytes().to_vec(),
        }
    }
}

/// Loads the static encryption key from `.encryption-key`, creating it when absent.
pub fn load_or_create_static_cipher(data_dir: &Path) -> anyhow::Result<StaticKeyCipher> {
    fs::create_dir_all(data_dir)
        .with_context(|| format!("failed to create data directory {data_dir:?}"))?;
    let key_path = data_dir.join(".encryption-key");
    let secret = match fs::read_to_string(&key_path) {
        Ok(secret) => secret,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            let generated = uuid::Uuid::new_v4().simple().to_string();
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&key_path) {
                Ok(mut file) => {
                    file.write_all(generated.as_bytes()).with_context(|| {
                        format!("failed to write encryption key to {key_path:?}")
                    })?;
                    generated
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    fs::read_to_string(&key_path).with_context(|| {
                        format!("failed to read encryption key from {key_path:?}")
                    })?
                }
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("failed to create encryption key at {key_path:?}")
                    });
                }
            }
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to read encryption key from {key_path:?}"));
        }
    };

    if secret.is_empty() {
        anyhow::bail!("encryption key at {key_path:?} is empty");
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("failed to secure encryption key at {key_path:?}"))?;
    }

    Ok(StaticKeyCipher::new(&secret))
}

impl KeyCipher for StaticKeyCipher {
    fn encrypt(&self, plaintext: &str) -> anyhow::Result<String> {
        xor_encrypt(plaintext, &self.seed)
    }

    fn decrypt(&self, ciphertext: &str) -> anyhow::Result<String> {
        xor_decrypt(ciphertext, &self.seed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("ocg-crypto-{name}-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn machine_bound_roundtrip() {
        let original = "sk-ocg-test-key-12345";
        let cipher = MachineBoundCipher::new();
        let encrypted = cipher.encrypt(original).unwrap();
        assert_ne!(encrypted, original);
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, original);
    }

    #[test]
    fn static_key_roundtrip() {
        let original = "sk-ocg-test-key-12345";
        let cipher = StaticKeyCipher::new("my-secret-key");
        let encrypted = cipher.encrypt(original).unwrap();
        assert_ne!(encrypted, original);
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, original);
    }

    // --- negative cases ---

    /// Same plaintext encrypted twice yields different ciphertext (nonce randomness).
    #[test]
    fn static_key_encrypt_is_nondeterministic() {
        let cipher = StaticKeyCipher::new("k");
        let a = cipher.encrypt("hello").unwrap();
        let b = cipher.encrypt("hello").unwrap();
        assert_ne!(a, b, "ciphertext must differ across calls (random nonce)");
    }

    /// Empty plaintext must round-trip to empty string — no panic, no base64 garbage.
    #[test]
    fn static_key_empty_string_roundtrip() {
        let cipher = StaticKeyCipher::new("k");
        let enc = cipher.encrypt("").unwrap();
        assert_eq!(enc, "");
        let dec = cipher.decrypt("").unwrap();
        assert_eq!(dec, "");
    }

    /// Static cipher on a non-empty plaintext is non-empty and not equal to plaintext.
    #[test]
    fn static_key_ciphertext_is_not_plaintext() {
        let cipher = StaticKeyCipher::new("k");
        let original = "sk-ocg-secret";
        let enc = cipher.encrypt(original).unwrap();
        assert!(!enc.is_empty());
        assert_ne!(enc, original);
        // base64 only — should not contain the plaintext substring
        assert!(!enc.contains("sk-ocg-secret"));
    }

    /// Different secrets cannot decrypt each other's ciphertext — the cross-cipher incompatibility
    /// that the README warns about for shared data dirs.
    #[test]
    fn static_key_wrong_secret_fails_to_decrypt() {
        let enc = StaticKeyCipher::new("right-key")
            .encrypt("payload")
            .unwrap();
        let result = StaticKeyCipher::new("wrong-key").decrypt(&enc);
        // XOR with a different key gives garbage; we accept either a UTF-8 error or a wrong-string
        // result, but the result must NOT equal the original payload.
        match result {
            Err(_) => {}                       // invalid utf-8 — fine
            Ok(s) => assert_ne!(s, "payload"), // decoded but wrong — still wrong
        }
    }

    /// Garbage base64 must error rather than panic.
    #[test]
    fn static_key_rejects_garbage_ciphertext() {
        let cipher = StaticKeyCipher::new("k");
        assert!(cipher.decrypt("!!!not-base64!!!").is_err());
    }

    /// Valid base64 but too short (< NONCE_LEN bytes) must error.
    #[test]
    fn static_key_rejects_short_ciphertext() {
        let cipher = StaticKeyCipher::new("k");
        // 4 bytes of valid base64 = "AAAA"
        assert!(cipher.decrypt("AAAA").is_err());
    }

    /// Two StaticKeyCiphers with the same secret must be interchangeable.
    #[test]
    fn static_key_same_secret_is_interchangeable() {
        let a = StaticKeyCipher::new("shared");
        let b = StaticKeyCipher::new("shared");
        let enc = a.encrypt("payload").unwrap();
        let dec = b.decrypt(&enc).unwrap();
        assert_eq!(dec, "payload");
    }

    #[test]
    fn static_key_file_is_created_and_reused() {
        let dir = test_dir("reuse");
        let first = load_or_create_static_cipher(&dir).unwrap();
        let key_path = dir.join(".encryption-key");
        let original = fs::read_to_string(&key_path).unwrap();
        let second = load_or_create_static_cipher(&dir).unwrap();

        assert!(!original.is_empty());
        assert_eq!(fs::read_to_string(&key_path).unwrap(), original);
        assert_eq!(
            second.decrypt(&first.encrypt("payload").unwrap()).unwrap(),
            "payload"
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn static_key_file_contents_are_preserved_exactly() {
        let dir = test_dir("preserve");
        fs::create_dir_all(&dir).unwrap();
        let key_path = dir.join(".encryption-key");
        fs::write(&key_path, " existing-secret\n").unwrap();

        let loaded = load_or_create_static_cipher(&dir).unwrap();
        let expected = StaticKeyCipher::new(" existing-secret\n");

        assert_eq!(fs::read_to_string(&key_path).unwrap(), " existing-secret\n");
        assert_eq!(
            loaded
                .decrypt(&expected.encrypt("payload").unwrap())
                .unwrap(),
            "payload"
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn static_key_file_rejects_empty_secret() {
        let dir = test_dir("empty");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(".encryption-key"), "").unwrap();

        let error = load_or_create_static_cipher(&dir).unwrap_err();

        assert!(error.to_string().contains("is empty"));
        fs::remove_dir_all(dir).unwrap();
    }
}
