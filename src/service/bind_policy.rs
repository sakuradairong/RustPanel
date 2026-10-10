//! Listen-address / TLS bootstrap policy (RP-10 / C.8).
//!
//! Plaintext HTTP on a non-loopback address is refused unless the operator
//! explicitly opts in with `RP_ALLOW_INSECURE_PUBLIC_BIND=1`. When
//! `RP_TLS_CERT` and `RP_TLS_KEY` are both set, public binds are allowed
//! (TLS expected at the process or an upstream that terminates TLS and
//! sets `RP_COOKIE_SECURE=1`).

use std::net::IpAddr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindPolicyError {
    InsecurePublicBind { host: String },
}

impl std::fmt::Display for BindPolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InsecurePublicBind { host } => write!(
                f,
                "Refusing plaintext HTTP bind on non-loopback address `{host}`. \
                 Bind to 127.0.0.1, configure TLS (`RP_TLS_CERT`/`RP_TLS_KEY`), \
                 or set RP_ALLOW_INSECURE_PUBLIC_BIND=1 for an explicit insecure override."
            ),
        }
    }
}

impl std::error::Error for BindPolicyError {}

fn env_truthy(name: &str) -> bool {
    matches!(
        std::env::var(name)
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

pub fn tls_files_configured() -> bool {
    let cert = std::env::var("RP_TLS_CERT").unwrap_or_default();
    let key = std::env::var("RP_TLS_KEY").unwrap_or_default();
    !cert.trim().is_empty() && !key.trim().is_empty()
}

pub fn allow_insecure_public_bind() -> bool {
    env_truthy("RP_ALLOW_INSECURE_PUBLIC_BIND")
}

pub fn is_loopback_host(host: &str) -> bool {
    let host = host.trim();
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match host.parse::<IpAddr>() {
        Ok(ip) => ip.is_loopback(),
        Err(_) => false,
    }
}

/// Cookie Secure flag: on when TLS files are configured, cookie env forces it,
/// or when binding publicly (defense in depth even behind a TLS proxy).
pub fn cookie_secure_recommended(bind_host: &str) -> bool {
    if env_truthy("RP_COOKIE_SECURE") {
        return true;
    }
    if std::env::var("RP_COOKIE_SECURE")
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "0" | "false" | "no" | "off"))
        .unwrap_or(false)
    {
        return false;
    }
    tls_files_configured() || !is_loopback_host(bind_host)
}

pub fn validate_bind_host(host: &str) -> Result<(), BindPolicyError> {
    if is_loopback_host(host) {
        return Ok(());
    }
    if tls_files_configured() || allow_insecure_public_bind() {
        return Ok(());
    }
    Err(BindPolicyError::InsecurePublicBind {
        host: host.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn loopback_always_ok() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var("RP_ALLOW_INSECURE_PUBLIC_BIND");
        std::env::remove_var("RP_TLS_CERT");
        std::env::remove_var("RP_TLS_KEY");
        assert!(validate_bind_host("127.0.0.1").is_ok());
        assert!(validate_bind_host("::1").is_ok());
        assert!(validate_bind_host("localhost").is_ok());
    }

    #[test]
    fn public_bind_refused_without_override() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var("RP_ALLOW_INSECURE_PUBLIC_BIND");
        std::env::remove_var("RP_TLS_CERT");
        std::env::remove_var("RP_TLS_KEY");
        assert!(matches!(
            validate_bind_host("0.0.0.0"),
            Err(BindPolicyError::InsecurePublicBind { .. })
        ));
        assert!(matches!(
            validate_bind_host("192.168.1.10"),
            Err(BindPolicyError::InsecurePublicBind { .. })
        ));
    }

    #[test]
    fn public_bind_allowed_with_override_or_tls_env() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var("RP_TLS_CERT");
        std::env::remove_var("RP_TLS_KEY");
        std::env::set_var("RP_ALLOW_INSECURE_PUBLIC_BIND", "1");
        assert!(validate_bind_host("0.0.0.0").is_ok());
        std::env::remove_var("RP_ALLOW_INSECURE_PUBLIC_BIND");
        std::env::set_var("RP_TLS_CERT", "/tmp/cert.pem");
        std::env::set_var("RP_TLS_KEY", "/tmp/key.pem");
        assert!(validate_bind_host("0.0.0.0").is_ok());
        std::env::remove_var("RP_TLS_CERT");
        std::env::remove_var("RP_TLS_KEY");
    }
}
