//! Required instance keys. Never substitute a shared key when configuration is missing.
use std::env;

pub struct SecurityKeys {
    pub signing: String,
    pub session: Vec<u8>,
}

impl SecurityKeys {
    pub fn from_environment() -> Result<Self, String> {
        let signing = env::var("RP_SECRET_KEY").map_err(|_| {
            "RP_SECRET_KEY must be set to an instance-specific random key".to_owned()
        })?;
        let session = env::var("RP_SESSION_KEY")
            .map_err(|_| "RP_SESSION_KEY must be set to an independent random key".to_owned())?;
        Self::validate(signing, session)
    }

    fn validate(signing: String, session: String) -> Result<Self, String> {
        if signing.len() < 32 || signing.chars().any(char::is_whitespace) {
            return Err("RP_SECRET_KEY must contain at least 32 bytes without whitespace".into());
        }
        if session.len() < 64 || session.chars().any(char::is_whitespace) {
            return Err("RP_SESSION_KEY must contain at least 64 bytes without whitespace".into());
        }
        if signing == session {
            return Err("RP_SECRET_KEY and RP_SESSION_KEY must be different".into());
        }
        // Explicitly configuring a formerly published default is unsafe too.
        if signing == "4e425cc46f42a4e01ee54a3a1e7db645e629621d"
            || session == "749d93ea30bf13e0b4a07d8f8fe5aa488b6c83fc886a266dd2c80590026f7b49"
        {
            return Err("Published legacy keys must be replaced with fresh random keys".into());
        }
        Ok(Self {
            signing,
            session: session.into_bytes(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::SecurityKeys;

    #[test]
    fn validates_lengths_without_exposing_key_values() {
        let secret = "short-private-value";
        let error = SecurityKeys::validate(secret.into(), "s".repeat(64))
            .err()
            .unwrap();
        assert!(error.contains("RP_SECRET_KEY"));
        assert!(!error.contains(secret));
        assert!(SecurityKeys::validate("j".repeat(32), "s".repeat(63)).is_err());
        assert!(SecurityKeys::validate("j".repeat(32), "s".repeat(64)).is_ok());
    }

    #[test]
    fn rejects_whitespace_duplicate_and_published_keys() {
        assert!(SecurityKeys::validate("j ".repeat(32), "s".repeat(64)).is_err());
        assert!(SecurityKeys::validate("j".repeat(64), "j".repeat(64)).is_err());
        assert!(SecurityKeys::validate(
            "4e425cc46f42a4e01ee54a3a1e7db645e629621d".into(),
            "s".repeat(64),
        )
        .is_err());
        assert!(SecurityKeys::validate(
            "j".repeat(32),
            "749d93ea30bf13e0b4a07d8f8fe5aa488b6c83fc886a266dd2c80590026f7b49".into(),
        )
        .is_err());
    }
}
