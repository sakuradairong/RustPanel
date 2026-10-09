use std::path::{Path, PathBuf};

use tokio::fs;
use tokio::process::Command;

use super::apache::{
    command_text, path_exists, remove_if_present, service_is_active, ManagedSite, WrittenSite,
};
use super::config::{self, SiteSpec};
use super::{io_err, SiteError};

const IMPORT_MARKER: &str = "# rustpanel-sites";

#[derive(Debug, Clone)]
pub struct CaddyLayout {
    pub binary: PathBuf,
    pub sites_dir: PathBuf,
    pub caddyfile: PathBuf,
    pub service: String,
}

pub async fn detect() -> Option<CaddyLayout> {
    if path_exists("./server/caddy/caddy").await {
        let root = PathBuf::from("./server/caddy");
        return Some(CaddyLayout {
            binary: root.join("caddy"),
            sites_dir: root.join("sites"),
            caddyfile: root.join("Caddyfile"),
            service: "caddy".into(),
        });
    }
    for binary in ["/usr/bin/caddy", "/usr/local/bin/caddy"] {
        if path_exists(binary).await {
            return Some(CaddyLayout {
                binary: PathBuf::from(binary),
                sites_dir: PathBuf::from("/etc/caddy/sites"),
                caddyfile: PathBuf::from("/etc/caddy/Caddyfile"),
                service: "caddy".into(),
            });
        }
    }
    None
}

pub async fn version(layout: &CaddyLayout) -> Option<String> {
    let mut command = Command::new(&layout.binary);
    command.arg("version");
    let (_ok, text) = command_text(command).await.ok()?;
    text.lines()
        .next()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
}

pub async fn is_running(layout: &CaddyLayout) -> bool {
    service_is_active(&layout.service).await
}

pub async fn test_config(layout: &CaddyLayout) -> Result<String, String> {
    let mut command = Command::new(&layout.binary);
    command
        .arg("validate")
        .arg("--config")
        .arg(&layout.caddyfile);
    let (ok, text) = command_text(command).await?;
    if ok {
        Ok(if text.is_empty() { "ok".into() } else { text })
    } else {
        Err(if text.is_empty() {
            "caddy configuration test failed".into()
        } else {
            text
        })
    }
}

pub async fn reload(layout: &CaddyLayout) -> Result<String, String> {
    let mut last_error = String::from("failed to reload caddy");
    let mut reload = Command::new(&layout.binary);
    reload.arg("reload").arg("--config").arg(&layout.caddyfile);
    match command_text(reload).await {
        Ok((true, text)) => {
            return Ok(if text.is_empty() {
                "caddy reloaded".into()
            } else {
                text
            });
        }
        Ok((false, text)) if !text.is_empty() => last_error = text,
        Err(err) => last_error = err,
        Ok((false, _)) => {}
    }
    let mut service = Command::new("systemctl");
    service.args(["reload", &layout.service]);
    match command_text(service).await {
        Ok((true, text)) => Ok(if text.is_empty() {
            "caddy reloaded".into()
        } else {
            text
        }),
        Ok((false, text)) if !text.is_empty() => Err(text),
        Err(err) => Err(err),
        Ok((false, _)) => Err(last_error),
    }
}

pub async fn list_sites(layout: &CaddyLayout) -> Result<Vec<ManagedSite>, SiteError> {
    let mut sites = Vec::new();
    let mut entries = match fs::read_dir(&layout.sites_dir).await {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(sites),
        Err(err) => return Err(io_err("failed to read caddy sites", err)),
    };
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|err| io_err("failed to read caddy sites", err))?
    {
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let (name, enabled) = if let Some(stem) = file_name.strip_suffix(".caddy.disabled") {
            (stem, false)
        } else if let Some(stem) = file_name.strip_suffix(".caddy") {
            (stem, true)
        } else {
            continue;
        };
        if config::validate_label(name).is_err() {
            continue;
        }
        let content = fs::read_to_string(&path)
            .await
            .map_err(|err| io_err("failed to read caddy site", err))?;
        let parsed = config::parse_caddy(&content);
        sites.push(ManagedSite {
            name: name.to_string(),
            path: path.to_string_lossy().to_string(),
            server_name: parsed.server_name,
            listen: parsed.listen,
            enabled,
            proxy_pass: parsed.proxy_pass,
            root: parsed.root,
            ssl: parsed.ssl,
        });
    }
    sites.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(sites)
}

pub async fn create_site(layout: &CaddyLayout, spec: &SiteSpec) -> Result<WrittenSite, SiteError> {
    let config_text = config::render_caddy(spec)?;
    let name = config::file_stem(&spec.server_name)?;
    fs::create_dir_all(&layout.sites_dir)
        .await
        .map_err(|err| io_err("failed to create caddy sites directory", err))?;
    let path = layout.sites_dir.join(format!("{name}.caddy"));
    let disabled = layout.sites_dir.join(format!("{name}.caddy.disabled"));
    if fs::symlink_metadata(&path).await.is_ok() || fs::symlink_metadata(&disabled).await.is_ok() {
        return Err(SiteError::Invalid(format!("site '{name}' already exists")));
    }
    fs::write(&path, &config_text)
        .await
        .map_err(|err| io_err("failed to write caddy site", err))?;
    ensure_import(layout).await?;
    Ok(WrittenSite {
        name,
        path: path.to_string_lossy().to_string(),
        config: config_text,
    })
}

pub async fn delete_site(layout: &CaddyLayout, name: &str) -> Result<(), SiteError> {
    let name = config::normalize_site_stem(name)?;
    let enabled = layout.sites_dir.join(format!("{name}.caddy"));
    let disabled = layout.sites_dir.join(format!("{name}.caddy.disabled"));
    let mut removed = remove_if_present(&enabled).await?;
    removed |= remove_if_present(&disabled).await?;
    if removed {
        Ok(())
    } else {
        Err(SiteError::NotFound(format!("site '{name}' not found")))
    }
}

pub async fn set_enabled(layout: &CaddyLayout, name: &str, enabled: bool) -> Result<(), SiteError> {
    let name = config::normalize_site_stem(name)?;
    let enabled_path = layout.sites_dir.join(format!("{name}.caddy"));
    let disabled_path = layout.sites_dir.join(format!("{name}.caddy.disabled"));
    if enabled {
        if fs::symlink_metadata(&enabled_path).await.is_ok() {
            return Ok(());
        }
        if fs::symlink_metadata(&disabled_path).await.is_err() {
            return Err(SiteError::NotFound(format!("site '{name}' not found")));
        }
        fs::rename(&disabled_path, &enabled_path)
            .await
            .map_err(|err| io_err("failed to enable caddy site", err))
    } else if fs::symlink_metadata(&disabled_path).await.is_ok() {
        Ok(())
    } else if fs::symlink_metadata(&enabled_path).await.is_err() {
        Err(SiteError::NotFound(format!("site '{name}' not found")))
    } else {
        fs::rename(&enabled_path, &disabled_path)
            .await
            .map_err(|err| io_err("failed to disable caddy site", err))
    }
}

pub async fn ensure_import(layout: &CaddyLayout) -> Result<(), SiteError> {
    if let Some(parent) = layout.caddyfile.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|err| io_err("failed to create caddy config directory", err))?;
    }
    let sites_dir = absolute_path(&layout.sites_dir);
    let import_line = format!("import {}/*.caddy", sites_dir.display());
    let existing = match fs::read_to_string(&layout.caddyfile).await {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(io_err("failed to read Caddyfile", err)),
    };
    if existing.contains(IMPORT_MARKER) || existing.contains(&import_line) {
        return Ok(());
    }
    let mut next = existing;
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    if !next.is_empty() {
        next.push('\n');
    }
    next.push_str(IMPORT_MARKER);
    next.push('\n');
    next.push_str(&import_line);
    next.push('\n');
    fs::write(&layout.caddyfile, next)
        .await
        .map_err(|err| io_err("failed to update Caddyfile", err))
}

fn absolute_path(path: &Path) -> PathBuf {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|current| current.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    let mut normalized = PathBuf::new();
    for component in joined.components() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::webserver::config::SslMode;

    #[tokio::test]
    async fn site_import_disable_and_delete() {
        let dir = tempfile::tempdir().unwrap();
        let layout = CaddyLayout {
            binary: dir.path().join("caddy"),
            sites_dir: dir.path().join("sites"),
            caddyfile: dir.path().join("Caddyfile"),
            service: "caddy".into(),
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

        create_site(&layout, &spec).await.unwrap();
        let caddyfile = fs::read_to_string(&layout.caddyfile).await.unwrap();
        assert!(caddyfile.contains("# rustpanel-sites"));
        assert!(caddyfile.contains("*.caddy"));
        let sites = list_sites(&layout).await.unwrap();
        assert_eq!(sites.len(), 1);
        assert!(sites[0].enabled);
        assert!(!sites[0].ssl);

        set_enabled(&layout, "example-com", false).await.unwrap();
        assert!(!list_sites(&layout).await.unwrap()[0].enabled);
        delete_site(&layout, "example-com").await.unwrap();
        assert!(list_sites(&layout).await.unwrap().is_empty());

        create_site(&layout, &spec).await.unwrap();
        let again = fs::read_to_string(&layout.caddyfile).await.unwrap();
        assert_eq!(again.matches("# rustpanel-sites").count(), 1);
    }

    #[tokio::test]
    async fn installer_marker_is_not_imported_twice() {
        let dir = tempfile::tempdir().unwrap();
        let sites = dir.path().join("sites");
        fs::create_dir_all(&sites).await.unwrap();
        let sites = fs::canonicalize(&sites).await.unwrap();
        let caddyfile = dir.path().join("Caddyfile");
        fs::write(
            &caddyfile,
            format!("# rustpanel-sites\nimport {}/*.caddy\n", sites.display()),
        )
        .await
        .unwrap();
        let layout = CaddyLayout {
            binary: dir.path().join("caddy"),
            sites_dir: sites,
            caddyfile,
            service: "caddy".into(),
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
        create_site(&layout, &spec).await.unwrap();
        let text = fs::read_to_string(&layout.caddyfile).await.unwrap();
        assert_eq!(text.matches("# rustpanel-sites").count(), 1);
        assert_eq!(text.matches("import ").count(), 1);
    }
}
