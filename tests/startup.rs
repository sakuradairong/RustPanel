//! Startup failures must occur before an initial account or credential is created.
use std::{fs, net::TcpListener, process::Command};

fn run_panel(host: &str, port: u16) -> (tempfile::TempDir, std::process::Output) {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("config")).unwrap();
    let config = include_str!("../config/conf.yaml.example")
        .replace("host: 127.0.0.1", &format!("host: {host}"))
        .replace("port: 8080", &format!("port: {port}"));
    fs::write(dir.path().join("config/conf.yaml"), config).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_panel"))
        .current_dir(dir.path())
        .env(
            "RP_SECRET_KEY",
            "startup-test-signing-key-at-least-32-bytes",
        )
        .env(
            "RP_SESSION_KEY",
            "startup-test-session-key-at-least-64-bytes-and-independent-of-signing-xx",
        )
        .env(
            "RP_MASTER_KEY",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .env("RP_TLS_CERT", "/nonexistent/cert")
        .env("RP_TLS_KEY", "/nonexistent/key")
        .env_remove("RP_ALLOW_INSECURE_PUBLIC_BIND")
        .output()
        .unwrap();
    (dir, output)
}

#[test]
fn certificate_environment_cannot_enable_public_plaintext_or_seed_an_admin() {
    let (dir, output) = run_panel("0.0.0.0", 0);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Refusing plaintext HTTP"));
    assert!(!dir.path().join("data.db3").exists());
    assert!(!dir.path().join("runtime/bootstrap-admin.txt").exists());
}

#[test]
fn occupied_port_is_rejected_before_bootstrap() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let (dir, output) = run_panel("127.0.0.1", listener.local_addr().unwrap().port());
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .to_lowercase()
        .contains("address already in use"));
    assert!(!dir.path().join("data.db3").exists());
    assert!(!dir.path().join("runtime/bootstrap-admin.txt").exists());
}
