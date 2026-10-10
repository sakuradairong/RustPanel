//! Password hashing: Argon2id for new secrets, SHA-1 verify for legacy rows.
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use rand::rngs::OsRng;

use crate::common::fun::sha1_salt;

pub const ALGO_SHA1: &str = "sha1";
pub const ALGO_ARGON2ID: &str = "argon2id";

#[derive(Debug)]
pub enum PasswordError {
    Hash,
    Verify,
}

impl std::fmt::Display for PasswordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hash => write!(f, "password hash failed"),
            Self::Verify => write!(f, "password verify failed"),
        }
    }
}

impl std::error::Error for PasswordError {}

/// Hash a password with Argon2id. The PHC string embeds salt and parameters.
pub fn hash_argon2id(password: &str) -> Result<String, PasswordError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| PasswordError::Hash)
}

pub fn verify_argon2id(password_hash: &str, password: &str) -> Result<bool, PasswordError> {
    let parsed = PasswordHash::new(password_hash).map_err(|_| PasswordError::Verify)?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

pub fn verify_sha1(password_hash: &str, salt: &str, password: &str) -> bool {
    use subtle::ConstantTimeEq;
    let expected = sha1_salt(password.to_owned(), salt.to_owned());
    password_hash.as_bytes().ct_eq(expected.as_bytes()).into()
}

/// Verify against the stored algorithm. Returns `(matches, should_rehash_to_argon2id)`.
pub fn verify_stored(
    password_hash: &str,
    salt: &str,
    algo: &str,
    password: &str,
) -> Result<(bool, bool), PasswordError> {
    match algo {
        ALGO_ARGON2ID => Ok((verify_argon2id(password_hash, password)?, false)),
        ALGO_SHA1 | "" => Ok((verify_sha1(password_hash, salt, password), true)),
        _ => Err(PasswordError::Verify),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argon2id_round_trip_and_rejects_wrong_password() {
        let hash = hash_argon2id("correct-horse").unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(verify_argon2id(&hash, "correct-horse").unwrap());
        assert!(!verify_argon2id(&hash, "wrong-password").unwrap());
    }

    #[test]
    fn sha1_legacy_verify_requests_rehash() {
        let salt = "legacy-salt";
        let hash = sha1_salt("legacy-secret".into(), salt.into());
        let (ok, rehash) = verify_stored(&hash, salt, ALGO_SHA1, "legacy-secret").unwrap();
        assert!(ok);
        assert!(rehash);
        let (ok, rehash) = verify_stored(&hash, salt, ALGO_SHA1, "nope").unwrap();
        assert!(!ok);
        assert!(rehash);
    }

    #[test]
    fn argon2id_stored_does_not_request_rehash() {
        let hash = hash_argon2id("fresh-secret").unwrap();
        let (ok, rehash) = verify_stored(&hash, "", ALGO_ARGON2ID, "fresh-secret").unwrap();
        assert!(ok);
        assert!(!rehash);
    }
}
