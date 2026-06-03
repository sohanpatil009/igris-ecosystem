use ed25519_dalek::pkcs8::spki::der::pem::LineEnding;
use ed25519_dalek::pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePrivateKey, EncodePublicKey};
use ed25519_dalek::{Signer, Verifier};
use sha2::{Digest, Sha256};

pub struct SigningTokenKey {
    inner: ed25519_dalek::SigningKey,
}

impl SigningTokenKey {
    pub fn to_verifying_key(&self) -> VerifyingTokenKey {
        VerifyingTokenKey {
            inner: self.inner.verifying_key(),
        }
    }
}

#[derive(Clone)]
pub struct VerifyingTokenKey {
    inner: ed25519_dalek::VerifyingKey,
}

impl VerifyingTokenKey {
    pub fn verify(&self, msg: &[u8], signature: &[u8]) -> anyhow::Result<()> {
        let signature = ed25519_dalek::Signature::from_slice(signature)?;
        self.inner.verify(msg, &signature)?;
        Ok(())
    }

    pub fn to_der(&self) -> anyhow::Result<Vec<u8>> {
        Ok(self.inner.to_public_key_der()?.into_vec())
    }
}

pub fn generate_key() -> SigningTokenKey {
    let mut csprng = rand::rngs::OsRng;
    SigningTokenKey {
        inner: ed25519_dalek::SigningKey::generate(&mut csprng),
    }
}

pub fn export_private_key(key: &SigningTokenKey) -> anyhow::Result<String> {
    let pem = key.inner.to_pkcs8_pem(LineEnding::LF)?;
    Ok(pem.to_string())
}

pub fn parse_private_key(private_key: &str) -> anyhow::Result<SigningTokenKey> {
    let parsed = ed25519_dalek::SigningKey::from_pkcs8_pem(private_key)?;
    Ok(SigningTokenKey { inner: parsed })
}

pub fn export_public_key(key: &SigningTokenKey) -> anyhow::Result<String> {
    let pem = key.inner.verifying_key().to_public_key_pem(LineEnding::LF)?;
    Ok(pem)
}

pub fn parse_public_key(public_key: &str) -> anyhow::Result<VerifyingTokenKey> {
    let inner = ed25519_dalek::VerifyingKey::from_public_key_pem(public_key)?;
    Ok(VerifyingTokenKey { inner })
}

fn sha256(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

fn base64_encode(data: &[u8]) -> String {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;
    STANDARD.encode(data)
}

fn base64_decode(data: &str) -> anyhow::Result<Vec<u8>> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;
    Ok(STANDARD.decode(data)?)
}

pub fn generate_token_timestamp(key: &SigningTokenKey) -> anyhow::Result<String> {
    let salt = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs()
        .to_le_bytes();
    generate_token_nonce(key, &salt)
}

pub fn generate_token_nonce(key: &SigningTokenKey, salt: &[u8]) -> anyhow::Result<String> {
    let public_key = key.inner.verifying_key().to_public_key_der()?;
    let hash_input = [public_key.as_bytes(), salt].concat();
    let digest = sha256(&hash_input);
    let signature = key.inner.sign(&digest);

    Ok(format!(
        "sha256.{}.{}.ed25519.{}",
        base64_encode(&digest),
        base64_encode(salt),
        base64_encode(signature.to_bytes().as_ref())
    ))
}

pub fn verify_token_timestamp(public_key: &VerifyingTokenKey, token: &str) -> bool {
    verify_token_with_result(public_key, token, |salt| {
        if salt.len() != 8 {
            return Err(anyhow::anyhow!("Invalid salt length"));
        }
        let salt_bytes: [u8; 8] = salt
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid salt"))?;
        let ts = u64::from_le_bytes(salt_bytes);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| anyhow::anyhow!("Time error: {}", e))?
            .as_secs();
        if now - ts > 3600 {
            return Err(anyhow::anyhow!("Token expired"));
        }
        Ok(())
    })
    .is_ok()
}

pub fn verify_token_nonce(
    public_key: &VerifyingTokenKey,
    token: &str,
    nonce: &[u8],
) -> bool {
    verify_token_with_result(public_key, token, |salt| {
        if salt != nonce {
            return Err(anyhow::anyhow!("Invalid nonce"));
        }
        Ok(())
    })
    .is_ok()
}

fn verify_token_with_result(
    public_key: &VerifyingTokenKey,
    token: &str,
    verify_salt: impl Fn(&[u8]) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let parts: Vec<&str> = token.split('.').collect();
    let [hash_method, hash_base64, salt_base64, sign_method, signature_base64] = parts[..] else {
        return Err(anyhow::anyhow!("Invalid token structure"));
    };

    if hash_method != "sha256" {
        return Err(anyhow::anyhow!("Invalid hash method"));
    }
    if sign_method != "ed25519" {
        return Err(anyhow::anyhow!("Invalid sign method"));
    }

    let salt = base64_decode(salt_base64)?;
    verify_salt(&salt)?;

    let public_key_der = public_key.to_der()?;
    let hash_input = [public_key_der.as_slice(), &salt].concat();
    let digest = sha256(&hash_input);

    if base64_encode(&digest) != hash_base64 {
        return Err(anyhow::anyhow!("Hash mismatch"));
    }

    let signature = base64_decode(signature_base64)?;
    public_key.verify(&digest, &signature)?;

    Ok(())
}
