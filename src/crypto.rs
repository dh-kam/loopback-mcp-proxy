use std::fs;
use std::path::Path;

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::error::AppError;

const NONCE_LEN: usize = 12;

pub struct Crypto {
    key: [u8; 32],
}

impl Crypto {
    pub fn load_or_create(path: &Path, env_key: Option<&str>) -> Result<Self, AppError> {
        if let Some(encoded) = env_key.filter(|value| !value.is_empty()) {
            let bytes = STANDARD
                .decode(encoded.trim())
                .map_err(|err| AppError::bad(format!("PROXY_DATA_KEY is not base64: {err}")))?;
            let key: [u8; 32] = bytes
                .as_slice()
                .try_into()
                .map_err(|_| AppError::bad("PROXY_DATA_KEY must decode to 32 bytes"))?;
            return Ok(Self { key });
        }
        if path.exists() {
            let bytes =
                fs::read(path).map_err(|err| AppError::internal(format!("read key: {err}")))?;
            let key: [u8; 32] = bytes
                .as_slice()
                .try_into()
                .map_err(|_| AppError::internal("master.key must be 32 bytes"))?;
            return Ok(Self { key });
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| AppError::internal(format!("create data dir: {err}")))?;
        }
        let mut key = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut key);
        fs::write(path, key).map_err(|err| AppError::internal(format!("write key: {err}")))?;
        restrict_private(path);
        Ok(Self { key })
    }

    pub fn encrypt(&self, plain: &str) -> Result<String, AppError> {
        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|err| AppError::internal(format!("cipher: {err}")))?;
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, plain.as_bytes())
            .map_err(|err| AppError::internal(format!("encrypt: {err}")))?;
        let mut packed = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        packed.extend_from_slice(&nonce_bytes);
        packed.extend_from_slice(&ciphertext);
        Ok(STANDARD.encode(packed))
    }

    pub fn decrypt(&self, stored: &str) -> Result<String, AppError> {
        let packed = STANDARD
            .decode(stored.trim())
            .map_err(|err| AppError::internal(format!("credential blob: {err}")))?;
        if packed.len() <= NONCE_LEN {
            return Err(AppError::internal("credential blob is truncated"));
        }
        let (nonce_bytes, ciphertext) = packed.split_at(NONCE_LEN);
        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|err| AppError::internal(format!("cipher: {err}")))?;
        let plain = cipher
            .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
            .map_err(|_| AppError::internal("credential blob could not be decrypted"))?;
        String::from_utf8(plain)
            .map_err(|err| AppError::internal(format!("credential utf-8: {err}")))
    }

    pub fn from_passphrase(
        passphrase: &str,
        salt_b64: Option<&str>,
    ) -> Result<(Self, String), AppError> {
        let pass = passphrase.trim();
        if pass.len() < 8 {
            return Err(AppError::bad(
                "passphrase must be at least 8 characters long",
            ));
        }
        let salt_bytes = if let Some(b64) = salt_b64 {
            STANDARD
                .decode(b64.trim())
                .map_err(|_| AppError::bad("invalid vault bundle salt"))?
        } else {
            let mut s = vec![0u8; 16];
            rand::rngs::OsRng.fill_bytes(&mut s);
            s
        };
        let mut derived = [0u8; 32];
        let mut hasher = Sha256::new();
        hasher.update(b"loopback-mcp-vault-v1:");
        hasher.update(&salt_bytes);
        hasher.update(b":");
        hasher.update(pass.as_bytes());
        derived.copy_from_slice(&hasher.finalize());
        for round in 0..20_000u32 {
            let mut h = Sha256::new();
            h.update(derived);
            h.update(&salt_bytes);
            h.update(round.to_le_bytes());
            derived.copy_from_slice(&h.finalize());
        }
        Ok((Self { key: derived }, STANDARD.encode(&salt_bytes)))
    }
}

fn restrict_private(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let crypto = Crypto { key: [7u8; 32] };
        let encrypted = crypto.encrypt("token-value").unwrap();
        assert_ne!(encrypted, "token-value");
        assert_eq!(crypto.decrypt(&encrypted).unwrap(), "token-value");
    }
}
