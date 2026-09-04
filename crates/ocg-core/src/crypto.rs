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

#[doc(inline)]
pub use ocg_infra::crypto::{
    KeyCipher, MachineBoundCipher, StaticKeyCipher, load_or_create_static_cipher,
};
