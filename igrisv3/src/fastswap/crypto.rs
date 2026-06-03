use aes_gcm::{Aes256Gcm, Key, Nonce};
use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};

pub type EncryptionKey = [u8; 32];

/// Generate a random 256-bit encryption key.
pub fn generate_key() -> EncryptionKey {
    let key = Aes256Gcm::generate_key(OsRng);
    key.into()
}

/// Encrypt plaintext with AES-256-GCM.
///
/// Output format: [12-byte nonce || ciphertext || 16-byte tag]
/// Each call generates a fresh random nonce, so encrypting the same data twice
/// produces different output.
pub fn encrypt(key: &EncryptionKey, plaintext: &[u8]) -> Vec<u8> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher.encrypt(&nonce, plaintext).expect("encryption failed");

    let mut result = Vec::with_capacity(12 + ciphertext.len());
    result.extend_from_slice(&nonce);
    result.extend_from_slice(&ciphertext);
    result
}

/// Decrypt data encrypted by [`encrypt`].
///
/// Input format: [12-byte nonce || ciphertext || 16-byte tag]
pub fn decrypt(key: &EncryptionKey, data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 12 {
        return Err("encrypted data too short for nonce".to_string());
    }
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let (nonce_bytes, ciphertext) = data.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("decryption failed: {}", e))
}
