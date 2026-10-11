//! Load and validate the panel master key (KEK).
use crypto::{digest::Digest, sha2::Sha256};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

/// AES-256 key length for the KEK.
pub const MASTER_KEY_BYTES: usize = 32;

const ENV_KEY: &str = "RP_MASTER_KEY";
const ENV_KEY_FILE: &str = "RP_MASTER_KEY_FILE";
const ENV_KEY_ID: &str = "RP_MASTER_KEY_ID";
const DEFAULT_KEY_FILE: &str = "/etc/rustpanel/master.key";

/// Active key-encryption key and its version id.
#[derive(Clone)]
pub struct MasterKey {
    bytes: [u8; MASTER_KEY_BYTES],
    key_id: String,
}

impl MasterKey {
    pub fn from_bytes(bytes: [u8; MASTER_KEY_BYTES], key_id: impl Into<String>) -> Self {
        Self {
            bytes,
            key_id: key_id.into(),
        }
    }

    /// Load from `RP_MASTER_KEY` (hex), `RP_MASTER_KEY_FILE`, or `/etc/rustpanel/master.key`.
    pub fn from_environment() -> Result<Self, String> {
        if let Ok(hex_value) = env::var(ENV_KEY) {
            let bytes = decode_key_material(&hex_value)?;
            let key_id = env::var(ENV_KEY_ID).unwrap_or_else(|_| derive_key_id(&bytes));
            return Ok(Self::from_bytes(bytes, key_id));
        }

        let path = env::var(ENV_KEY_FILE)
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(DEFAULT_KEY_FILE));

        if !path.exists() {
            return Err(format!(
                "{ENV_KEY} must be set (64 hex chars) or provide {ENV_KEY_FILE} / {DEFAULT_KEY_FILE}"
            ));
        }

        let material = read_key_file(&path)?;
        let bytes = decode_key_material(&material)?;
        let key_id = env::var(ENV_KEY_ID).unwrap_or_else(|_| derive_key_id(&bytes));
        Ok(Self::from_bytes(bytes, key_id))
    }

    pub fn bytes(&self) -> &[u8; MASTER_KEY_BYTES] {
        &self.bytes
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }
}

fn read_key_file(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(|_| {
        format!(
            "Failed to read master key file {}; check path and permissions (0600)",
            path.display()
        )
    })?;
    let metadata = file
        .metadata()
        .map_err(|_| "Cannot inspect master key file".to_owned())?;
    if !metadata.is_file() {
        return Err("Master key must be a regular file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.mode() & 0o7777 != 0o600 || metadata.uid() != unsafe { libc::geteuid() } {
            return Err("Master key file must be owned by the process user with mode 0600".into());
        }
    }
    let mut raw = Vec::new();
    file.take(129)
        .read_to_end(&mut raw)
        .map_err(|_| "Cannot read master key file".to_owned())?;
    if raw.len() > 128 {
        return Err("Master key file is too large".into());
    }
    // Prefer hex text; also accept raw 32-byte files.
    if raw.len() == MASTER_KEY_BYTES && !looks_like_hex_text(&raw) {
        return Ok(hex_encode(&raw));
    }
    let text = String::from_utf8(raw).map_err(|_| {
        format!(
            "Master key file {} must be UTF-8 hex or exactly {MASTER_KEY_BYTES} raw bytes",
            path.display()
        )
    })?;
    Ok(text.trim().to_owned())
}

fn looks_like_hex_text(raw: &[u8]) -> bool {
    raw.iter()
        .all(|b| b.is_ascii_whitespace() || b.is_ascii_hexdigit())
}

fn decode_key_material(value: &str) -> Result<[u8; MASTER_KEY_BYTES], String> {
    let trimmed = value.trim();
    if trimmed.chars().any(char::is_whitespace) {
        return Err(format!(
            "{ENV_KEY} must contain exactly 64 hex characters without whitespace"
        ));
    }
    if trimmed.len() != MASTER_KEY_BYTES * 2 {
        return Err(format!(
            "{ENV_KEY} must be exactly {} hex characters (32 bytes)",
            MASTER_KEY_BYTES * 2
        ));
    }
    if !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("{ENV_KEY} must be hexadecimal"));
    }
    let decoded = hex_decode(trimmed)?;
    let mut bytes = [0u8; MASTER_KEY_BYTES];
    bytes.copy_from_slice(&decoded);
    // Reject obviously weak all-zero keys.
    if bytes.iter().all(|b| *b == 0) {
        return Err(format!("{ENV_KEY} must not be all zeros"));
    }
    Ok(bytes)
}

fn derive_key_id(bytes: &[u8; MASTER_KEY_BYTES]) -> String {
    let mut hasher = Sha256::new();
    hasher.input(bytes);
    let digest = hasher.result_str();
    format!("kek-{}", &digest[..16])
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(hex: &str) -> Result<Vec<u8>, String> {
    if hex.len() % 2 != 0 {
        return Err(format!("{ENV_KEY} hex length must be even"));
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&hex[i..i + 2], 16)
                .map_err(|_| format!("{ENV_KEY} contains invalid hex"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    const SAMPLE: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn accepts_hex_env_material() {
        let key = MasterKey::from_bytes(decode_key_material(SAMPLE).unwrap(), "test");
        assert_eq!(key.bytes().len(), 32);
        assert_eq!(key.key_id(), "test");
    }

    #[test]
    fn rejects_short_whitespace_and_zero_keys() {
        assert!(decode_key_material("abcd").is_err());
        assert!(decode_key_material(&format!("{} {}", &SAMPLE[..32], &SAMPLE[32..])).is_err());
        assert!(decode_key_material(&"0".repeat(64)).is_err());
        let err = decode_key_material("short-private-value").unwrap_err();
        assert!(err.contains(ENV_KEY));
        assert!(!err.contains("short-private-value"));
    }

    #[test]
    fn reads_raw_and_hex_key_files() {
        let mut hex_file = NamedTempFile::new().unwrap();
        writeln!(hex_file, "{SAMPLE}").unwrap();
        let material = read_key_file(hex_file.path()).unwrap();
        assert_eq!(decode_key_material(&material).unwrap().len(), 32);

        let mut raw_file = NamedTempFile::new().unwrap();
        let raw = decode_key_material(SAMPLE).unwrap();
        raw_file.write_all(&raw).unwrap();
        let material = read_key_file(raw_file.path()).unwrap();
        assert_eq!(decode_key_material(&material).unwrap(), raw);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_public_key_permissions_symlinks_and_nonregular_files() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let file = NamedTempFile::new().unwrap();
        fs::write(file.path(), SAMPLE).unwrap();
        fs::set_permissions(file.path(), fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read_key_file(file.path()).is_err());
        fs::set_permissions(file.path(), fs::Permissions::from_mode(0o600)).unwrap();
        assert!(read_key_file(file.path()).is_ok());
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("key");
        symlink(file.path(), &link).unwrap();
        assert!(read_key_file(&link).is_err());
        assert!(read_key_file(dir.path()).is_err());
    }

    #[test]
    fn derives_stable_key_id() {
        let bytes = decode_key_material(SAMPLE).unwrap();
        let a = derive_key_id(&bytes);
        let b = derive_key_id(&bytes);
        assert_eq!(a, b);
        assert!(a.starts_with("kek-"));
    }
}
