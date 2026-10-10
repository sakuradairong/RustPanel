use std::error::Error;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use tokio::fs;
use tokio::process::Command;

use super::manager::OsManager;

pub struct CaddyInstaller;

const DEFAULT_VERSION: &str = "2.11.7";
const UNIT_MARKER: &str = "# rustpanel-caddy";
const CADDYFILE_MARKER: &str = "# rustpanel-sites";

impl CaddyInstaller {
    pub async fn install(version: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        let version = normalize_version(version)?;
        let arch = caddy_arch()?;
        let url = format!(
            "https://github.com/caddyserver/caddy/releases/download/v{version}/caddy_{version}_linux_{arch}.tar.gz"
        );

        let temp_dir = std::env::temp_dir().join(format!("rustpanel_caddy_{version}"));
        if temp_dir.exists() {
            fs::remove_dir_all(&temp_dir).await?;
        }
        fs::create_dir_all(&temp_dir).await?;
        let archive = temp_dir.join(format!("caddy_{version}_linux_{arch}.tar.gz"));
        OsManager::download_file(&url, &archive).await?;
        OsManager::extract_tarball(&archive, &temp_dir).await?;

        let extracted = temp_dir.join("caddy");
        if !extracted.exists() {
            return Err("Caddy archive did not contain the caddy binary".into());
        }

        let install_dir = std::env::current_dir()?.join("server").join("caddy");
        fs::create_dir_all(&install_dir).await?;
        let binary = install_dir.join("caddy");
        if fs::rename(&extracted, &binary).await.is_err() {
            fs::copy(&extracted, &binary).await?;
        }
        fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).await?;

        let sites_dir = install_dir.join("sites");
        fs::create_dir_all(&sites_dir).await?;
        let caddyfile = install_dir.join("Caddyfile");
        if !caddyfile.exists() {
            let import = format!(
                "{CADDYFILE_MARKER}\nimport {}/*.caddy\n",
                fs::canonicalize(&sites_dir).await?.display()
            );
            fs::write(&caddyfile, import).await?;
        }

        let binary = fs::canonicalize(&binary).await?;
        let caddyfile = fs::canonicalize(&caddyfile).await?;
        write_service(&binary, &caddyfile).await?;
        let _ = fs::remove_dir_all(&temp_dir).await;
        Ok(())
    }

    pub async fn uninstall() -> Result<(), Box<dyn Error + Send + Sync>> {
        let unit = PathBuf::from("/etc/systemd/system/caddy.service");
        if let Ok(text) = fs::read_to_string(&unit).await {
            if text.contains(UNIT_MARKER) {
                let _ = Command::new("systemctl")
                    .args(["disable", "--now", "caddy"])
                    .status()
                    .await;
                fs::remove_file(&unit).await?;
                let _ = Command::new("systemctl")
                    .arg("daemon-reload")
                    .status()
                    .await;
            }
        }
        let install_dir = std::env::current_dir()?.join("server").join("caddy");
        if install_dir.exists() {
            fs::remove_dir_all(install_dir).await?;
        }
        Ok(())
    }
}

pub fn normalize_version(version: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
    let version = version.trim().trim_start_matches('v');
    let version = if version.is_empty() || version == "latest" {
        DEFAULT_VERSION
    } else {
        version
    };
    let pieces: Vec<&str> = version.split('.').collect();
    let numeric = pieces.len() == 3
        && pieces.iter().all(|piece| {
            !piece.is_empty() && piece.len() <= 8 && piece.chars().all(|c| c.is_ascii_digit())
        });
    if numeric {
        Ok(version.to_string())
    } else {
        Err(format!("unsupported Caddy version '{version}'").into())
    }
}

fn caddy_arch() -> Result<&'static str, Box<dyn Error + Send + Sync>> {
    match std::env::consts::ARCH {
        "x86_64" => Ok("amd64"),
        "aarch64" => Ok("arm64"),
        "arm" => Ok("armv7"),
        other => Err(format!("unsupported architecture '{other}'").into()),
    }
}

async fn write_service(
    binary: &std::path::Path,
    caddyfile: &std::path::Path,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let unit_path = PathBuf::from("/etc/systemd/system/caddy.service");
    if let Ok(text) = fs::read_to_string(&unit_path).await {
        if !text.contains(UNIT_MARKER) {
            return Err(
                "a Caddy systemd unit already exists and was not created by RustPanel".into(),
            );
        }
    }
    let unit = format!(
        r#"{UNIT_MARKER}
[Unit]
Description=Caddy
After=network-online.target
Wants=network-online.target

[Service]
ExecStart="{bin}" run --config "{config}"
ExecReload="{bin}" reload --config "{config}"
TimeoutStopSec=5s
LimitNOFILE=1048576
Restart=on-failure

[Install]
WantedBy=multi-user.target
"#,
        bin = binary.display(),
        config = caddyfile.display()
    );
    fs::write(&unit_path, unit).await?;
    let _ = Command::new("systemctl")
        .arg("daemon-reload")
        .status()
        .await;
    let _ = Command::new("systemctl")
        .args(["enable", "--now", "caddy"])
        .status()
        .await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_latest_and_numeric_versions() {
        assert_eq!(normalize_version("latest").unwrap(), DEFAULT_VERSION);
        assert_eq!(normalize_version("v2.11.7").unwrap(), "2.11.7");
        assert!(normalize_version("2.11").is_err());
        assert!(normalize_version("2.11.7;rm").is_err());
    }
}
