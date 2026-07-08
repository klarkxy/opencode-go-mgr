//! Simple machine-bound obfuscation for API keys.
//!
//! This is intentionally lightweight: keys are not stored in plain text on disk,
//! but the scheme is NOT a substitute for a real KMS or AES-GCM. If stronger
//! security is needed later, replace this module with `ring`/`aes-gcm`.

use base64::{engine::general_purpose::STANDARD, Engine};
use std::env;

const NONCE_LEN: usize = 16;

fn machine_seed() -> Vec<u8> {
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

pub fn encrypt(plaintext: &str) -> anyhow::Result<String> {
    if plaintext.is_empty() {
        return Ok(String::new());
    }
    let bytes = plaintext.as_bytes();
    // Use UUID v4 for a random 16-byte nonce (per-call random, not deterministic)
    let nonce: Vec<u8> = uuid::Uuid::new_v4().as_bytes().to_vec();
    let key = derive_key(&machine_seed(), bytes.len());
    let mut cipher = Vec::with_capacity(NONCE_LEN + bytes.len());
    cipher.extend_from_slice(&nonce);
    for (i, b) in bytes.iter().enumerate() {
        cipher.push(b ^ key[i] ^ nonce[i % NONCE_LEN]);
    }
    Ok(STANDARD.encode(&cipher))
}

pub fn decrypt(ciphertext: &str) -> anyhow::Result<String> {
    if ciphertext.is_empty() {
        return Ok(String::new());
    }
    let cipher = STANDARD.decode(ciphertext)?;
    if cipher.len() < NONCE_LEN {
        anyhow::bail!("invalid cipher text");
    }
    let (nonce, body) = cipher.split_at(NONCE_LEN);
    let key = derive_key(&machine_seed(), body.len());
    let mut plain = Vec::with_capacity(body.len());
    for (i, b) in body.iter().enumerate() {
        plain.push(b ^ key[i] ^ nonce[i % NONCE_LEN]);
    }
    String::from_utf8(plain).map_err(|e| anyhow::anyhow!(e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let original = "sk-ocg-test-key-12345";
        let cipher = encrypt(original).unwrap();
        assert_ne!(cipher, original);
        let decrypted = decrypt(&cipher).unwrap();
        assert_eq!(decrypted, original);
    }
}
