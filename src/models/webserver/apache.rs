use std::path::{Path, PathBuf};

use serde::Serialize;
use tokio::fs;
use tokio::process::Command;

use super::config::{self, ParsedSite, SiteSpec};
use super::{io_err, SiteError};

#[derive(Debug, Clone)]
pub struct ApacheLayout {
    pub binary: PathBuf,
    pub available: PathBuf,
    pub enabled: PathBuf,
    pub services: Vec<String>,
    pub ctl: Option<PathBuf>,
    /// httpd has no sites-enabled directory. This file, under conf.d, only
    /// includes the panel's own sites-enabled directory.
    pub include_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManagedSite {
    pub name: String,
    pub path: String,
    pub server_name: Option<String>,
    pub listen: Option<String>,
    pub enabled: bool,
    pub proxy_pass: Option<String>,
    pub root: Option<String>,
    pub ssl: bool,
}

#[derive(Debug, Clone)]
pub struct WrittenSite {
    pub name: String,
    pub path: String,
    pub config: String,
}

impl From<ParsedSite> for SiteFields {
    fn from(parsed: ParsedSite) -> Self {
        Self {
            server_name: parsed.server_name,
            listen: parsed.listen,
            proxy_pass: parsed.proxy_pass,
            root: parsed.root,
            ssl: parsed.ssl,
        }
    }
}

struct SiteFields {
    server_name: Option<String>,
    listen: Option<String>,
    proxy_pass: Option<String>,
    root: Option<String>,
    ssl: bool,
}

pub async fn detect() -> Option<ApacheLayout> {
    if let Some(binary) =
        first_existing(&["./server/apache/bin/httpd", "./server/apache/bin/apache2"]).await
    {
        let root = PathBuf::from("./server/apache");
        return Some(ApacheLayout {
            binary,
            available: root.join("sites-available"),
            enabled: root.join("sites-enabled"),
            services: vec!["httpd".into(), "apache2".into()],
            ctl: None,
            include_file: None,
        });
    }

    if path_exists("/etc/apache2").await
        && (path_exists("/usr/sbin/apache2").await || path_exists("/usr/sbin/apache2ctl").await)
    {
        let binary = if path_exists("/usr/sbin/apache2").await {
            PathBuf::from("/usr/sbin/apache2")
        } else {
            PathBuf::from("/usr/sbin/apache2ctl")
        };
        return Some(ApacheLayout {
            binary,
            available: PathBuf::from("/etc/apache2/sites-available"),
            enabled: PathBuf::from("/etc/apache2/sites-enabled"),
            services: vec!["apache2".into()],
            ctl: existing("/usr/sbin/apache2ctl").await,
            include_file: None,
        });
    }

    if path_exists("/usr/sbin/httpd").await {
        return Some(ApacheLayout {
            binary: PathBuf::from("/usr/sbin/httpd"),
            available: PathBuf::from("/etc/httpd/sites-available"),
            enabled: PathBuf::from("/etc/httpd/sites-enabled"),
            services: vec!["httpd".into()],
            ctl: existing("/usr/sbin/apachectl").await,
            include_file: Some(PathBuf::from("/etc/httpd/conf.d/rustpanel.conf")),
        });
    }

    None
}

pub async fn version(layout: &ApacheLayout) -> Option<String> {
    let mut command = Command::new(&layout.binary);
    command.arg("-v");
    let (_ok, text) = command_text(command).await.ok()?;
    text.lines()
        .find(|line| line.contains("Server version"))
        .map(|line| line.trim().to_string())
}

pub async fn is_running(layout: &ApacheLayout) -> bool {
    for service in &layout.services {
        if service_is_active(service).await {
            return true;
        }
    }
    false
}

pub async fn test_config(layout: &ApacheLayout) -> Result<String, String> {
    let mut command = if let Some(ctl) = &layout.ctl {
        Command::new(ctl)
    } else {
        Command::new(&layout.binary)
    };
    if layout.ctl.is_some() {
        command.arg("configtest");
    } else {
        command.arg("-t");
    }
    let (ok, text) = command_text(command).await?;
    if ok {
        Ok(if text.is_empty() { "ok".into() } else { text })
    } else {
        Err(if text.is_empty() {
            "apache configuration test failed".into()
        } else {
            text
        })
    }
}

pub async fn reload(layout: &ApacheLayout) -> Result<String, String> {
    let mut last_error = String::from("failed to reload apache");
    let mut graceful = Command::new(&layout.binary);
    graceful.args(["-k", "graceful"]);
    match command_text(graceful).await {
        Ok((true, text)) => {
            return Ok(if text.is_empty() {
                "apache reloaded".into()
            } else {
                text
            });
        }
        Ok((false, text)) if !text.is_empty() => last_error = text,
        Err(err) => last_error = err,
        Ok((false, _)) => {}
    }
    for service in &layout.services {
        let mut reload = Command::new("systemctl");
        reload.args(["reload", service]);
        match command_text(reload).await {
            Ok((true, text)) => {
                return Ok(if text.is_empty() {
                    format!("{service} reloaded")
                } else {
                    text
                });
            }
            Ok((false, text)) if !text.is_empty() => last_error = text,
            Err(err) => last_error = err,
            Ok((false, _)) => {}
        }
    }
    Err(last_error)
}

pub async fn list_sites(layout: &ApacheLayout) -> Result<Vec<ManagedSite>, SiteError> {
    let mut sites = Vec::new();
    let mut entries = match fs::read_dir(&layout.available).await {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(sites),
        Err(err) => return Err(io_err("failed to read apache sites", err)),
    };
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|err| io_err("failed to read apache sites", err))?
    {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("conf") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        if config::validate_label(name).is_err() {
            continue;
        }
        let content = fs::read_to_string(&path)
            .await
            .map_err(|err| io_err("failed to read apache site", err))?;
        let parsed = config::parse_apache(&content);
        let enabled = fs::symlink_metadata(layout.enabled.join(format!("{name}.conf")))
            .await
            .is_ok();
        sites.push(site_from_parsed(name, &path, enabled, parsed));
    }
    sites.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(sites)
}

pub async fn create_site(layout: &ApacheLayout, spec: &SiteSpec) -> Result<WrittenSite, SiteError> {
    let config = config::render_apache(spec)?;
    let name = config::file_stem(&spec.server_name)?;
    fs::create_dir_all(&layout.available)
        .await
        .map_err(|err| io_err("failed to create apache sites directory", err))?;
    fs::create_dir_all(&layout.enabled)
        .await
        .map_err(|err| io_err("failed to create apache sites directory", err))?;
    let path = layout.available.join(format!("{name}.conf"));
    if fs::symlink_metadata(&path).await.is_ok() {
        return Err(SiteError::Invalid(format!("site '{name}' already exists")));
    }
    fs::write(&path, &config)
        .await
        .map_err(|err| io_err("failed to write apache site", err))?;
    ensure_include(layout).await?;
    link_enabled(layout, &name).await?;
    Ok(WrittenSite {
        name,
        path: path.to_string_lossy().to_string(),
        config,
    })
}

pub async fn delete_site(layout: &ApacheLayout, name: &str) -> Result<(), SiteError> {
    let name = config::normalize_site_stem(name)?;
    let available = layout.available.join(format!("{name}.conf"));
    let enabled = layout.enabled.join(format!("{name}.conf"));
    let mut removed = remove_if_present(&enabled).await?;
    removed |= remove_if_present(&available).await?;
    if removed {
        Ok(())
    } else {
        Err(SiteError::NotFound(format!("site '{name}' not found")))
    }
}

pub async fn set_enabled(
    layout: &ApacheLayout,
    name: &str,
    enabled: bool,
) -> Result<(), SiteError> {
    let name = config::normalize_site_stem(name)?;
    let available = layout.available.join(format!("{name}.conf"));
    if fs::symlink_metadata(&available).await.is_err() {
        return Err(SiteError::NotFound(format!("site '{name}' not found")));
    }
    let link = layout.enabled.join(format!("{name}.conf"));
    if enabled {
        fs::create_dir_all(&layout.enabled)
            .await
            .map_err(|err| io_err("failed to create apache sites directory", err))?;
        ensure_include(layout).await?;
        link_enabled(layout, &name).await
    } else if remove_if_present(&link).await? {
        Ok(())
    } else {
        Err(SiteError::NotFound(format!("site '{name}' is not enabled")))
    }
}

const APACHE_INCLUDE_MARKER: &str = "# rustpanel-apache";

async fn ensure_include(layout: &ApacheLayout) -> Result<(), SiteError> {
    let Some(include_file) = &layout.include_file else {
        return Ok(());
    };
    if let Some(parent) = include_file.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|err| io_err("failed to create httpd include directory", err))?;
    }
    let enabled = normalize_path(&absolute_path(&layout.enabled));
    let body = format!(
        "{APACHE_INCLUDE_MARKER}\nIncludeOptional {}/*.conf\n",
        enabled.display()
    );
    match fs::read_to_string(include_file).await {
        Ok(text) if text.contains(APACHE_INCLUDE_MARKER) => Ok(()),
        Ok(_) => Err(SiteError::Invalid(
            "refusing to replace an existing httpd include".into(),
        )),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => fs::write(include_file, body)
            .await
            .map_err(|err| io_err("failed to write httpd include", err)),
        Err(err) => Err(io_err("failed to read httpd include", err)),
    }
}

async fn link_enabled(layout: &ApacheLayout, name: &str) -> Result<(), SiteError> {
    let source = layout.available.join(format!("{name}.conf"));
    let metadata = fs::symlink_metadata(&source)
        .await
        .map_err(|err| io_err("failed to inspect apache site", err))?;
    if !metadata.file_type().is_file() {
        return Err(SiteError::Invalid(format!(
            "site '{name}' must be a regular file in sites-available"
        )));
    }
    let available_root = fs::canonicalize(&layout.available)
        .await
        .map_err(|err| io_err("failed to resolve apache sites directory", err))?;
    let source_path = fs::canonicalize(&source)
        .await
        .map_err(|err| io_err("failed to resolve apache site", err))?;
    if !source_path.starts_with(&available_root) {
        return Err(SiteError::Invalid(
            "site file is outside the apache sites directory".into(),
        ));
    }
    fs::create_dir_all(&layout.enabled)
        .await
        .map_err(|err| io_err("failed to create apache sites directory", err))?;
    let link = layout.enabled.join(format!("{name}.conf"));
    if fs::symlink_metadata(&link).await.is_ok() {
        fs::remove_file(&link)
            .await
            .map_err(|err| io_err("failed to replace apache site link", err))?;
    }
    std::os::unix::fs::symlink(format!("../sites-available/{name}.conf"), &link)
        .map_err(|err| io_err("failed to enable apache site", err))
}

fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|current| current.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn site_from_parsed(name: &str, path: &Path, enabled: bool, parsed: ParsedSite) -> ManagedSite {
    let fields = SiteFields::from(parsed);
    ManagedSite {
        name: name.to_string(),
        path: path.to_string_lossy().to_string(),
        server_name: fields.server_name,
        listen: fields.listen,
        enabled,
        proxy_pass: fields.proxy_pass,
        root: fields.root,
        ssl: fields.ssl,
    }
}

pub(crate) async fn command_text(mut command: Command) -> Result<(bool, String), String> {
    let output = command.output().await.map_err(|err| err.to_string())?;
    let mut text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let err_text = String::from_utf8_lossy(&output.stderr);
    let err_text = err_text.trim();
    if !err_text.is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(err_text);
    }
    Ok((output.status.success(), text))
}

pub(crate) async fn service_is_active(name: &str) -> bool {
    let mut command = Command::new("systemctl");
    command.args(["is-active", name]);
    match command_text(command).await {
        Ok((_, text)) => text.lines().next().unwrap_or("").trim() == "active",
        Err(_) => false,
    }
}

pub(crate) async fn path_exists(path: impl AsRef<Path>) -> bool {
    fs::metadata(path.as_ref()).await.is_ok()
}

async fn first_existing(paths: &[&str]) -> Option<PathBuf> {
    for path in paths {
        if path_exists(path).await {
            return Some(PathBuf::from(path));
        }
    }
    None
}

async fn existing(path: &str) -> Option<PathBuf> {
    if path_exists(path).await {
        Some(PathBuf::from(path))
    } else {
        None
    }
}

pub(crate) async fn remove_if_present(path: &Path) -> Result<bool, SiteError> {
    match fs::symlink_metadata(path).await {
        Ok(_) => {
            fs::remove_file(path)
                .await
                .map_err(|err| io_err("failed to remove site file", err))?;
            Ok(true)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(io_err("failed to inspect site file", err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::webserver::config::SslMode;

    #[tokio::test]
    async fn site_can_be_created_disabled_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let layout = ApacheLayout {
            binary: dir.path().join("httpd"),
            available: dir.path().join("available"),
            enabled: dir.path().join("enabled"),
            services: vec!["apache2".into()],
            ctl: None,
            include_file: None,
        };
        let spec = SiteSpec {
            server_name: "example.com".into(),
            root: Some("/var/www/example".into()),
            listen: 80,
            proxy_pass: None,
            ssl: SslMode::None,
            index: "index.html".into(),
            extra_config: None,
        };

        let written = create_site(&layout, &spec).await.unwrap();
        assert_eq!(written.name, "example-com");
        let link = std::fs::read_link(layout.enabled.join("example-com.conf")).unwrap();
        assert_eq!(link, PathBuf::from("../sites-available/example-com.conf"));
        let sites = list_sites(&layout).await.unwrap();
        assert_eq!(sites.len(), 1);
        assert!(sites[0].enabled);
        assert_eq!(sites[0].root.as_deref(), Some("/var/www/example"));

        set_enabled(&layout, "example-com", false).await.unwrap();
        assert!(!list_sites(&layout).await.unwrap()[0].enabled);

        delete_site(&layout, "example-com.conf").await.unwrap();
        assert!(list_sites(&layout).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn enable_refuses_symlink_and_delete_leaves_other_configs() {
        let dir = tempfile::tempdir().unwrap();
        let layout = ApacheLayout {
            binary: dir.path().join("httpd"),
            available: dir.path().join("sites-available"),
            enabled: dir.path().join("sites-enabled"),
            services: vec!["httpd".into()],
            ctl: None,
            include_file: Some(dir.path().join("conf.d").join("rustpanel.conf")),
        };
        fs::create_dir_all(&layout.available).await.unwrap();
        let packaged = dir.path().join("conf.d").join("welcome.conf");
        fs::create_dir_all(packaged.parent().unwrap())
            .await
            .unwrap();
        fs::write(&packaged, "packaged\n").await.unwrap();
        std::os::unix::fs::symlink("/etc/hosts", layout.available.join("welcome.conf")).unwrap();

        assert!(set_enabled(&layout, "welcome", true).await.is_err());
        assert!(!layout.enabled.join("welcome.conf").exists());
        delete_site(&layout, "welcome").await.unwrap();
        assert!(!layout.available.join("welcome.conf").exists());
        assert_eq!(fs::read_to_string(&packaged).await.unwrap(), "packaged\n");
    }
}
