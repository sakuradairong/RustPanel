/*
 * @Descripttion: Nginx web server management API handlers
 * @version:
 * @Author: Wynters
 * @Date: 2024-05-25 16:23:02
 * @LastEditTime: 2024-06-02 10:00:00
 * @FilePath: \RustPanel\src\api\v1\webserver.rs
 */

use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::process::Command;

use crate::api::auth::AuthUser;
use crate::api::{ResponseStructure, ResponseStructureError};

#[derive(Deserialize)]
pub struct NginxSitePath {
    pub name: String,
}

#[derive(Deserialize)]
pub struct CreateSiteBody {
    pub server_name: String,
    pub root: String,
    pub listen: Option<u16>,
    pub proxy_pass: Option<String>,
    pub ssl: Option<bool>,
}

#[derive(Serialize)]
pub struct SiteInfo {
    pub name: String,
    pub path: String,
    pub server_name: Option<String>,
    pub listen: Option<String>,
    pub enabled: bool,
}

#[derive(Serialize)]
pub struct WebserverStatus {
    pub installed: bool,
    pub running: bool,
    pub version: Option<String>,
    pub config_test: String,
}

const NGINX_PATHS: &[&str] = &[
    "./server/nginx/sbin/nginx",
    "/usr/sbin/nginx",
    "/usr/local/nginx/sbin/nginx",
];

async fn find_nginx() -> Option<String> {
    for path in NGINX_PATHS {
        if fs::metadata(path).await.is_ok() {
            return Some(path.to_string());
        }
    }
    None
}

async fn is_local_install() -> bool {
    fs::metadata(NGINX_PATHS[0]).await.is_ok()
}

async fn run_nginx_test(nginx_path: &str) -> String {
    let output = Command::new(nginx_path).arg("-t").output().await;
    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let combined = if stdout.is_empty() { stderr } else { stdout };
            combined
        }
        Err(err) => format!("Failed to run nginx test: {}", err),
    }
}

async fn get_nginx_version(nginx_path: &str) -> Option<String> {
    let output = Command::new(nginx_path).arg("-v").output().await.ok()?;
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    // nginx -v outputs to stderr: "nginx version: nginx/1.18.0"
    if stderr.contains("nginx version:") {
        let version = stderr.trim().to_string();
        Some(version)
    } else {
        None
    }
}

async fn reload_nginx_internal(nginx_path: &str) -> Result<String, String> {
    // Try nginx -s reload first, fall back to systemctl reload
    let output = Command::new(nginx_path)
        .args(["-s", "reload"])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        return Ok("nginx reloaded".to_string());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !stderr.is_empty() {
        return Err(stderr);
    }

    // Fallback: try systemctl
    let output = Command::new("systemctl")
        .args(["reload", "nginx"])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok("nginx reloaded".to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Err(if stderr.is_empty() {
            "failed to reload nginx".to_string()
        } else {
            stderr
        })
    }
}

fn is_linux() -> bool {
    cfg!(target_os = "linux")
}

pub async fn get_webserver_status(_: AuthUser) -> HttpResponse {
    if !is_linux() {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false,
            code: 400,
            message: String::from("Web server management is only supported on Linux"),
        });
    }

    let nginx_path = find_nginx().await;
    let installed = nginx_path.is_some();

    let (version, config_test) = if let Some(ref path) = nginx_path {
        (
            get_nginx_version(path).await,
            run_nginx_test(path).await,
        )
    } else {
        (None, String::from("nginx not found"))
    };

    // Check if nginx service is running
    let running = if installed {
        let output = Command::new("systemctl")
            .args(["is-active", "nginx"])
            .output()
            .await;
        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                stdout.trim() == "active"
            }
            Err(_) => false,
        }
    } else {
        false
    };

    HttpResponse::Ok().json(ResponseStructure {
        success: true,
        code: 200,
        message: String::from("success"),
        data: Some(WebserverStatus {
            installed,
            running,
            version,
            config_test,
        }),
    })
}

pub async fn list_nginx_sites(_: AuthUser) -> HttpResponse {
    if !is_linux() {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false,
            code: 400,
            message: String::from("Web server management is only supported on Linux"),
        });
    }

    let mut sites: Vec<SiteInfo> = Vec::new();

    if is_local_install().await {
        // Local install: scan ./server/nginx/conf/
        let conf_dir = "./server/nginx/conf/";
        match fs::read_dir(conf_dir).await {
            Ok(mut entries) => {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("conf") {
                        let name = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_string();
                        let (server_name, listen) = parse_nginx_config(&path).await;
                        sites.push(SiteInfo {
                            name,
                            path: path.to_string_lossy().to_string(),
                            server_name,
                            listen,
                            enabled: true,
                        });
                    }
                }
            }
            Err(err) => {
                return HttpResponse::InternalServerError().json(ResponseStructureError {
                    success: false,
                    code: 500,
                    message: format!("Failed to read config directory: {}", err),
                });
            }
        }
    } else {
        // System nginx: scan /etc/nginx/conf.d/ and /etc/nginx/sites-enabled/
        let dirs = ["/etc/nginx/conf.d/", "/etc/nginx/sites-enabled/"];

        // Collect paths in sites-enabled for enabled check
        let mut enabled_paths: Vec<std::path::PathBuf> = Vec::new();
        if let Ok(mut entries) = fs::read_dir("/etc/nginx/sites-enabled/").await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                enabled_paths.push(entry.path());
            }
        }

        for dir in &dirs {
            match fs::read_dir(dir).await {
                Ok(mut entries) => {
                    while let Ok(Some(entry)) = entries.next_entry().await {
                        let path = entry.path();
                        if path.extension().and_then(|s| s.to_str()) == Some("conf")
                            || path.extension().is_none()
                        {
                            let name = path
                                .file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or("")
                                .to_string();
                            let (server_name, listen) = parse_nginx_config(&path).await;
                            let enabled = enabled_paths.iter().any(|p| *p == path)
                                || path.starts_with("/etc/nginx/sites-enabled/");
                            sites.push(SiteInfo {
                                name,
                                path: path.to_string_lossy().to_string(),
                                server_name,
                                listen,
                                enabled,
                            });
                        }
                    }
                }
                Err(_) => {
                    // Directory may not exist; skip silently
                }
            }
        }
    }

    HttpResponse::Ok().json(ResponseStructure {
        success: true,
        code: 200,
        message: String::from("success"),
        data: Some(sites),
    })
}

async fn parse_nginx_config(path: &std::path::Path) -> (Option<String>, Option<String>) {
    let content = fs::read_to_string(path).await.unwrap_or_default();
    let mut server_name: Option<String> = None;
    let mut listen: Option<String> = None;

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip comments
        if trimmed.starts_with('#') {
            continue;
        }

        // Parse server_name
        if server_name.is_none() {
            if let Some(val) = trimmed.strip_prefix("server_name") {
                let val = val.trim().trim_end_matches(';').trim();
                if !val.is_empty() {
                    // Take the first server_name value if multiple
                    server_name = Some(val.split_whitespace().next().unwrap_or(val).to_string());
                }
            }
        }

        // Parse listen
        if listen.is_none() {
            if let Some(val) = trimmed.strip_prefix("listen") {
                let val = val.trim().trim_end_matches(';').trim();
                if !val.is_empty() {
                    listen = Some(val.to_string());
                }
            }
        }

        // Stop if we found both
        if server_name.is_some() && listen.is_some() {
            break;
        }
    }

    (server_name, listen)
}

pub async fn create_nginx_site(
    _: AuthUser,
    body: web::Json<CreateSiteBody>,
) -> HttpResponse {
    if !is_linux() {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false,
            code: 400,
            message: String::from("Web server management is only supported on Linux"),
        });
    }

    let nginx_path = match find_nginx().await {
        Some(path) => path,
        None => {
            return HttpResponse::BadRequest().json(ResponseStructureError {
                success: false,
                code: 400,
                message: String::from("nginx is not installed"),
            });
        }
    };

    let listen_port = body.listen.unwrap_or(80);
    let ssl = body.ssl.unwrap_or(false);

    let mut config = String::new();
    config.push_str("server {\n");

    if ssl {
        config.push_str(&format!("    listen {} ssl;\n", listen_port));
        config.push_str(&format!(
            "    ssl_certificate /etc/nginx/ssl/{}.crt;\n",
            body.server_name
        ));
        config.push_str(&format!(
            "    ssl_certificate_key /etc/nginx/ssl/{}.key;\n",
            body.server_name
        ));
    } else {
        config.push_str(&format!("    listen {};\n", listen_port));
    }

    config.push_str(&format!("    server_name {};\n", body.server_name));

    if let Some(ref proxy_pass) = body.proxy_pass {
        config.push_str(&format!(
            "    location / {{\n        proxy_pass {};\n        proxy_set_header Host $host;\n        proxy_set_header X-Real-IP $remote_addr;\n        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n        proxy_set_header X-Forwarded-Proto $scheme;\n    }}\n",
            proxy_pass
        ));
    } else {
        config.push_str(&format!("    root {};\n", body.root));
        config.push_str("    index index.html index.htm;\n");
    }

    config.push_str("}\n");

    // Determine config directory and write
    let conf_dir = if is_local_install().await {
        "./server/nginx/conf/".to_string()
    } else {
        "/etc/nginx/conf.d/".to_string()
    };

    // Use server_name as the filename (replace dots with hyphens for safety)
    let filename = format!("{}.conf", body.server_name.replace('.', "-"));

    // Create directory if it doesn't exist (for local install)
    if let Err(err) = fs::create_dir_all(&conf_dir).await {
        return HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: format!("Failed to create config directory: {}", err),
        });
    }

    let config_path = format!("{}{}", conf_dir, filename);
    if let Err(err) = fs::write(&config_path, &config).await {
        return HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: format!("Failed to write config file: {}", err),
        });
    }

    // Test nginx configuration
    let test_result = run_nginx_test(&nginx_path).await;

    // Check if nginx -t failed by looking for "failed" or "error" in output
    let test_passed = !test_result.to_lowercase().contains("failed")
        && !test_result.to_lowercase().contains("error")
        && !test_result.to_lowercase().contains("emerg");

    if !test_passed {
        // Clean up the config file if test failed
        let _ = fs::remove_file(&config_path).await;

        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false,
            code: 400,
            message: format!("nginx configuration test failed: {}", test_result),
        });
    }

    // Reload nginx
    match reload_nginx_internal(&nginx_path).await {
        Ok(msg) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some(serde_json::json!({
                "config": config,
                "path": config_path,
                "reload": msg,
            })),
        }),
        Err(err) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("Config created but nginx reload failed"),
            data: Some(serde_json::json!({
                "config": config,
                "path": config_path,
                "reload_error": err,
            })),
        }),
    }
}

pub async fn delete_nginx_site(
    _: AuthUser,
    path: web::Path<NginxSitePath>,
) -> HttpResponse {
    if !is_linux() {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false,
            code: 400,
            message: String::from("Web server management is only supported on Linux"),
        });
    }

    let nginx_path = match find_nginx().await {
        Some(path) => path,
        None => {
            return HttpResponse::BadRequest().json(ResponseStructureError {
                success: false,
                code: 400,
                message: String::from("nginx is not installed"),
            });
        }
    };

    let name = &path.name;

    // Search for the config file in known locations
    let search_dirs = if is_local_install().await {
        vec!["./server/nginx/conf/".to_string()]
    } else {
        vec![
            "/etc/nginx/conf.d/".to_string(),
            "/etc/nginx/sites-enabled/".to_string(),
            "/etc/nginx/sites-available/".to_string(),
        ]
    };

    let mut deleted = false;

    for dir in &search_dirs {
        // Try exact name
        let exact_path = format!("{}{}", dir, name);
        if fs::metadata(&exact_path).await.is_ok() {
            if fs::remove_file(&exact_path).await.is_ok() {
                deleted = true;
            }
            continue;
        }

        // Try with .conf extension
        let conf_path = format!("{}{}.conf", dir, name);
        if fs::metadata(&conf_path).await.is_ok() {
            if fs::remove_file(&conf_path).await.is_ok() {
                deleted = true;
            }
        }
    }

    if !deleted {
        return HttpResponse::NotFound().json(ResponseStructureError {
            success: false,
            code: 404,
            message: format!("Site '{}' not found", name),
        });
    }

    // Test nginx configuration
    let test_result = run_nginx_test(&nginx_path).await;
    let test_passed = !test_result.to_lowercase().contains("failed")
        && !test_result.to_lowercase().contains("error")
        && !test_result.to_lowercase().contains("emerg");

    if !test_passed {
        return HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("Site deleted but nginx config test failed"),
            data: Some(serde_json::json!({
                "config_test": test_result,
            })),
        });
    }

    match reload_nginx_internal(&nginx_path).await {
        Ok(msg) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some(serde_json::json!({
                "reload": msg,
            })),
        }),
        Err(err) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("Site deleted but nginx reload failed"),
            data: Some(serde_json::json!({
                "reload_error": err,
            })),
        }),
    }
}

pub async fn reload_nginx(_: AuthUser) -> HttpResponse {
    if !is_linux() {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false,
            code: 400,
            message: String::from("Web server management is only supported on Linux"),
        });
    }

    let nginx_path = match find_nginx().await {
        Some(path) => path,
        None => {
            return HttpResponse::BadRequest().json(ResponseStructureError {
                success: false,
                code: 400,
                message: String::from("nginx is not installed"),
            });
        }
    };

    match reload_nginx_internal(&nginx_path).await {
        Ok(msg) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some(serde_json::json!({
                "reload": msg,
            })),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: err,
        }),
    }
}
