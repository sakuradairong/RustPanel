//! AES-256-GCM envelope: random DEK encrypts payload; KEK wraps the DEK.
use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use rand::RngCore;

use super::kek::{MasterKey, MASTER_KEY_BYTES};

const NONCE_LEN: usize = 12;

/// Persisted ciphertext bundle (no plaintext fields).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedSecret {
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
    pub wrapped_dek: Vec<u8>,
    pub wrap_nonce: Vec<u8>,
    pub key_id: String,
}

fn aad_bytes(purpose: &str, name: &str) -> Vec<u8> {
    format!("rp-secret|{purpose}|{name}").into_bytes()
}

fn random_nonce() -> [u8; NONCE_LEN] {
    let mut nonce = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce);
    nonce
}

/// Encrypt `plaintext` under a fresh DEK; wrap that DEK with `kek`.
pub fn encrypt(
    kek: &MasterKey,
    purpose: &str,
    name: &str,
    plaintext: &[u8],
) -> Result<SealedSecret, String> {
    let mut dek = [0u8; MASTER_KEY_BYTES];
    rand::thread_rng().fill_bytes(&mut dek);
    let data_nonce = random_nonce();
    let aad = aad_bytes(purpose, name);

    let data_cipher = Aes256Gcm::new_from_slice(&dek).map_err(|_| "Invalid DEK".to_owned())?;
    let ciphertext = data_cipher
        .encrypt(
            Nonce::from_slice(&data_nonce),
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| "Failed to encrypt secret".to_owned())?;

    let wrap_nonce = random_nonce();
    let wrap_cipher =
        Aes256Gcm::new_from_slice(kek.bytes()).map_err(|_| "Invalid KEK".to_owned())?;
    let wrapped_dek = wrap_cipher
        .encrypt(Nonce::from_slice(&wrap_nonce), dek.as_ref())
        .map_err(|_| "Failed to wrap DEK".to_owned())?;

    Ok(SealedSecret {
        ciphertext,
        nonce: data_nonce.to_vec(),
        wrapped_dek,
        wrap_nonce: wrap_nonce.to_vec(),
        key_id: kek.key_id().to_owned(),
    })
}

/// Decrypt a sealed secret using the provided KEK (must match `sealed.key_id` material).
pub fn decrypt(
    kek: &MasterKey,
    purpose: &str,
    name: &str,
    sealed: &SealedSecret,
) -> Result<Vec<u8>, String> {
    if sealed.nonce.len() != NONCE_LEN || sealed.wrap_nonce.len() != NONCE_LEN {
        return Err("Sealed secret has invalid nonce length".into());
    }

    let wrap_cipher =
        Aes256Gcm::new_from_slice(kek.bytes()).map_err(|_| "Invalid KEK".to_owned())?;
    let dek = wrap_cipher
        .decrypt(
            Nonce::from_slice(&sealed.wrap_nonce),
            sealed.wrapped_dek.as_ref(),
        )
        .map_err(|_| "Failed to unwrap DEK (wrong master key or corrupt blob)".to_owned())?;

    let aad = aad_bytes(purpose, name);
    let data_cipher = Aes256Gcm::new_from_slice(&dek).map_err(|_| "Invalid DEK".to_owned())?;
    data_cipher
        .decrypt(
            Nonce::from_slice(&sealed.nonce),
            Payload {
                msg: &sealed.ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| "Failed to decrypt secret (tampered or wrong purpose/name)".to_owned())
}

/// Re-wrap an existing DEK under `new_kek` without changing ciphertext.
pub fn rewrap(old_kek: &MasterKey, new_kek: &MasterKey, sealed: &SealedSecret) -> Result<SealedSecret, String> {
    if sealed.wrap_nonce.len() != NONCE_LEN {
        return Err("Sealed secret has invalid wrap nonce length".into());
    }
    let wrap_cipher =
        Aes256Gcm::new_from_slice(old_kek.bytes()).map_err(|_| "Invalid old KEK".to_owned())?;
    let dek = wrap_cipher
        .decrypt(
            Nonce::from_slice(&sealed.wrap_nonce),
            sealed.wrapped_dek.as_ref(),
        )
        .map_err(|_| "Failed to unwrap DEK for rotation".to_owned())?;

    let wrap_nonce = random_nonce();
    let new_wrap =
        Aes256Gcm::new_from_slice(new_kek.bytes()).map_err(|_| "Invalid new KEK".to_owned())?;
    let wrapped_dek = new_wrap
        .encrypt(Nonce::from_slice(&wrap_nonce), dek.as_ref())
        .map_err(|_| "Failed to re-wrap DEK".to_owned())?;

    Ok(SealedSecret {
        ciphertext: sealed.ciphertext.clone(),
        nonce: sealed.nonce.clone(),
        wrapped_dek,
        wrap_nonce: wrap_nonce.to_vec(),
        key_id: new_kek.key_id().to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_kek(id: &str) -> MasterKey {
        let mut bytes = [0u8; MASTER_KEY_BYTES];
        bytes.copy_from_slice(&[0x11u8; MASTER_KEY_BYTES]);
        bytes[0] = id.as_bytes()[0];
        MasterKey::from_bytes(bytes, id)
    }

    #[test]
    fn round_trip_and_aad_binding() {
        let kek = sample_kek("v1");
        let sealed = encrypt(&kek, "db", "mysql.root", b"s3cret-value").unwrap();
        assert!(!sealed.ciphertext.is_empty());
        assert_ne!(sealed.ciphertext, b"s3cret-value");
        let plain = decrypt(&kek, "db", "mysql.root", &sealed).unwrap();
        assert_eq!(plain, b"s3cret-value");
        assert!(decrypt(&kek, "db", "other", &sealed).is_err());
        assert!(decrypt(&kek, "acme", "mysql.root", &sealed).is_err());
    }

    #[test]
    fn wrong_kek_cannot_decrypt() {
        let kek_a = sample_kek("a");
        let kek_b = sample_kek("b");
        let sealed = encrypt(&kek_a, "s3", "access", b"payload").unwrap();
        assert!(decrypt(&kek_b, "s3", "access", &sealed).is_err());
    }

    #[test]
    fn rotation_preserves_plaintext_without_rewriting_ciphertext() {
        let old = sample_kek("old");
        let new = sample_kek("new");
        let sealed = encrypt(&old, "db", "pg.admin", b"rotate-me").unwrap();
        let original_ct = sealed.ciphertext.clone();
        let rotated = rewrap(&old, &new, &sealed).unwrap();
        assert_eq!(rotated.ciphertext, original_ct);
        assert_ne!(rotated.wrapped_dek, sealed.wrapped_dek);
        assert_eq!(rotated.key_id, "new");
        assert_eq!(decrypt(&new, "db", "pg.admin", &rotated).unwrap(), b"rotate-me");
        assert!(decrypt(&old, "db", "pg.admin", &rotated).is_err());
    }
}
