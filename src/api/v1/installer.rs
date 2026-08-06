use std::collections::HashMap;
use std::time::Duration;

use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};

use crate::api::auth::AuthUser;
use crate::api::{ResponseStructure, ResponseStructureError};
use crate::models::system::installer::{self, SoftwareType};

#[derive(Serialize)]
struct SoftwareInfo {
    name: String,
    description: String,
    versions_available: bool,
}

pub async fn list_software(_: AuthUser) -> HttpResponse {
    let software = vec![
        SoftwareInfo {
            name: String::from("nginx"),
            description: String::from("High-performance HTTP server and reverse proxy"),
            versions_available: true,
        },
        SoftwareInfo {
            name: String::from("mysql"),
            description: String::from("MySQL relational database management system"),
            versions_available: true,
        },
        SoftwareInfo {
            name: String::from("pgsql"),
            description: String::from("PostgreSQL object-relational database system"),
            versions_available: true,
        },
        SoftwareInfo {
            name: String::from("redis"),
            description: String::from("Redis in-memory data structure store"),
            versions_available: true,
        },
        SoftwareInfo {
            name: String::from("docker"),
            description: String::from("Docker container runtime"),
            versions_available: false,
        },
    ];

    HttpResponse::Ok().json(ResponseStructure {
        success: true,
        code: 200,
        message: String::from("success"),
        data: Some(software),
    })
}

/// Extract version strings that sit between `prefix` and `suffix` in `html`,
/// keeping only ones made of digits and dots (e.g. "1.27.0").
fn extract_versions(html: &str, prefix: &str, suffix: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find(prefix) {
        let after = &rest[i + prefix.len()..];
        if let Some(j) = after.find(suffix) {
            let v = &after[..j];
            if !v.is_empty()
                && v.len() <= 16
                && v.chars().all(|c| c.is_ascii_digit() || c == '.')
                && v.contains('.')
            {
                out.push(v.to_string());
            }
            rest = &after[j + suffix.len()..];
        } else {
            break;
        }
    }
    out
}

/// Sort version strings in descending (newest-first) order and de-duplicate.
fn sort_versions(mut versions: Vec<String>) -> Vec<String> {
    fn key(v: &str) -> Vec<u64> {
        v.split('.')
            .map(|p| p.parse::<u64>().unwrap_or(0))
            .collect()
    }
    versions.sort_by(|a, b| key(b).cmp(&key(a)));
    versions.dedup();
    versions
}

async fn fetch_upstream(url: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .user_agent("RustPanel/0.0.1")
        .build()
        .map_err(|e| e.to_string())?;
    client
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())
}

/// GET /installer/versions?software=nginx
/// Returns the list of versions available from the software's upstream source.
pub async fn list_versions(_: AuthUser, query: web::Query<HashMap<String, String>>) -> HttpResponse {
    let software = query.get("software").cloned().unwrap_or_default().to_lowercase();

    // (index url, prefix, suffix) per software; mysql has no simple listing.
    let cfg: Option<(&str, &str, &str)> = match software.as_str() {
        "nginx" => Some(("https://nginx.org/download/", "nginx-", ".tar.gz")),
        "redis" => Some(("https://download.redis.io/releases/", "redis-", ".tar.gz")),
        "docker" => Some((
            "https://download.docker.com/linux/static/stable/x86_64/",
            "docker-",
            ".tgz",
        )),
        "pgsql" => Some(("https://ftp.postgresql.org/pub/source/", "href=\"v", "/\"")),
        _ => None,
    };

    let Some((url, prefix, suffix)) = cfg else {
        // Unknown / unsupported (e.g. mysql): return an empty list so the UI
        // can fall back to manual version entry.
        return HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("no upstream version index"),
            data: Some(Vec::<String>::new()),
        });
    };

    match fetch_upstream(url).await {
        Ok(body) => {
            let mut versions = sort_versions(extract_versions(&body, prefix, suffix));
            versions.truncate(40);
            HttpResponse::Ok().json(ResponseStructure {
                success: true,
                code: 200,
                message: String::from("success"),
                data: Some(versions),
            })
        }
        Err(e) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: format!("Failed to fetch versions: {}", e),
        }),
    }
}

#[derive(Deserialize)]
pub struct InstallBody {
    pub software: String,
    pub version: Option<String>,
}

pub async fn install_software(_: AuthUser, body: web::Json<InstallBody>) -> HttpResponse {
    let software_type = match body.software.to_lowercase().as_str() {
        "nginx" => SoftwareType::Nginx,
        "mysql" => SoftwareType::Mysql,
        "pgsql" => SoftwareType::Pgsql,
        "redis" => SoftwareType::Redis,
        "docker" => SoftwareType::Docker,
        _ => {
            return HttpResponse::BadRequest().json(ResponseStructureError {
                success: false,
                code: 400,
                message: String::from("Unknown software type"),
            });
        }
    };

    let version = body.version.clone().unwrap_or_else(|| String::from("latest"));

    // Spawn blocking task since installation may take a while
    let result = web::block(move || {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(installer::install(software_type, &version))
    })
    .await;

    match result {
        Ok(Ok(())) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("Installation completed successfully"),
            data: None::<bool>,
        }),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: format!("Installation failed: {}", e),
        }),
        Err(e) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: format!("Installation task failed: {}", e),
        }),
    }
}

#[derive(Deserialize)]
pub struct UninstallBody {
    pub software: String,
}

pub async fn uninstall_software(_: AuthUser, body: web::Json<UninstallBody>) -> HttpResponse {
    let software_type = match body.software.to_lowercase().as_str() {
        "nginx" => SoftwareType::Nginx,
        "mysql" => SoftwareType::Mysql,
        "pgsql" => SoftwareType::Pgsql,
        "redis" => SoftwareType::Redis,
        "docker" => SoftwareType::Docker,
        _ => {
            return HttpResponse::BadRequest().json(ResponseStructureError {
                success: false,
                code: 400,
                message: String::from("Unknown software type"),
            });
        }
    };

    let result = web::block(move || {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(installer::uninstall(software_type))
    })
    .await;

    match result {
        Ok(Ok(())) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("Uninstallation completed successfully"),
            data: None::<bool>,
        }),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: format!("Uninstallation failed: {}", e),
        }),
        Err(e) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: format!("Uninstallation task failed: {}", e),
        }),
    }
}
