use std::error::Error;

use tokio::process::Command;

use super::manager::{OsManager, PackageManager};

pub struct ApacheInstaller;

impl ApacheInstaller {
    pub async fn install(_version: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        let (package, service) = package_and_service()?;
        let packages: &[&str] = if package == "httpd" {
            &["httpd", "mod_ssl"]
        } else {
            &[package]
        };
        OsManager::install_dependencies(packages).await?;
        if package == "apache2" {
            for module in ["proxy", "proxy_http", "ssl"] {
                let status = Command::new("a2enmod").arg(module).status().await?;
                if !status.success() {
                    return Err(format!("failed to enable Apache module {module}").into());
                }
            }
        }
        let status = Command::new("systemctl")
            .args(["enable", "--now", service])
            .status()
            .await?;
        if status.success() {
            Ok(())
        } else {
            Err(
                format!("Apache packages were installed, but service '{service}' did not start")
                    .into(),
            )
        }
    }

    pub async fn uninstall() -> Result<(), Box<dyn Error + Send + Sync>> {
        let (package, service) = package_and_service()?;
        let _ = Command::new("systemctl")
            .args(["disable", "--now", service])
            .status()
            .await;
        if package == "httpd" {
            OsManager::remove_packages(&["httpd", "mod_ssl"]).await
        } else {
            OsManager::remove_packages(&[package]).await
        }
    }
}

fn package_and_service() -> Result<(&'static str, &'static str), Box<dyn Error + Send + Sync>> {
    match OsManager::detect_package_manager() {
        PackageManager::Apt => Ok(("apache2", "apache2")),
        PackageManager::Yum => Ok(("httpd", "httpd")),
        PackageManager::Unknown => Err("Apache installation needs apt or yum".into()),
    }
}
