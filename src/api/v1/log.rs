/*
 * @Descripttion:
 * @version:
 * @Author: Wynters
 * @Date: 2024-05-25 16:23:02
 * @FilePath: \RustPanel\src\api\v1\log.rs
 */

use crate::api::auth::AuthUser;
use crate::api::{ResponseStructure, ResponseStructureError};
use crate::service::db::DBPool;
use crate::service::path_bounds;
use crate::service::rbac::{self, PERM_LOG_READ};
use actix_web::{web, HttpResponse, ResponseError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::fs;

/// Roots the log reader may access: the panel's own runtime/logs plus
/// /var/log. Override with RP_LOG_ROOTS (colon-separated absolute paths).
fn log_roots() -> Vec<PathBuf> {
    if let Ok(raw) = std::env::var("RP_LOG_ROOTS") {
        let roots: Vec<PathBuf> = raw
            .split(':')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .collect();
        if !roots.is_empty() {
            return roots;
        }
    }
    let runtime_logs = std::env::current_dir()
        .map(|cwd| cwd.join("runtime/logs"))
        .unwrap_or_else(|_| PathBuf::from("./runtime/logs"));
    vec![runtime_logs, PathBuf::from("/var/log")]
}

/// Hard cap on buffered log lines so a huge log file cannot exhaust memory.
const MAX_LOG_LINES: usize = 200_000;

#[derive(Deserialize)]
pub struct ReadLogQuery {
    pub path: String,
    pub lines: Option<usize>,
    pub keyword: Option<String>,
}

#[derive(Serialize)]
struct LogFileEntry {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub modified: u64,
}

pub async fn list_logs(user: AuthUser, pool: web::Data<DBPool>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_LOG_READ) {
        return err.error_response();
    }
    let mut files = Vec::new();

    // Scan ./runtime/logs/ for .log files
    if let Ok(mut entries) = fs::read_dir("./runtime/logs/").await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("log") {
                if let (Some(name), Ok(metadata)) = (
                    path.file_name().and_then(|n| n.to_str()),
                    entry.metadata().await,
                ) {
                    let modified = metadata
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    files.push(LogFileEntry {
                        // Absolute so the path matches a log root at read time.
                        path: path
                            .canonicalize()
                            .unwrap_or_else(|_| path.clone())
                            .to_string_lossy()
                            .to_string(),
                        name: name.to_string(),
                        size: metadata.len(),
                        modified,
                    });
                }
            }
        }
    }

    // Check common log files in /var/log/
    let common_logs = [
        "/var/log/syslog",
        "/var/log/auth.log",
        "/var/log/kern.log",
        "/var/log/dmesg",
        "/var/log/dpkg.log",
        "/var/log/faillog",
        "/var/log/lastlog",
        "/var/log/wtmp",
        "/var/log/btmp",
        "/var/log/messages",
        "/var/log/secure",
    ];

    for log_path in &common_logs {
        if Path::new(log_path).exists() {
            if let Some(name) = Path::new(log_path).file_name().and_then(|n| n.to_str()) {
                if let Ok(metadata) = fs::metadata(log_path).await {
                    let modified = metadata
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    files.push(LogFileEntry {
                        path: log_path.to_string(),
                        name: name.to_string(),
                        size: metadata.len(),
                        modified,
                    });
                }
            }
        }
    }

    HttpResponse::Ok().json(ResponseStructure {
        success: true,
        code: 200,
        message: String::from("success"),
        data: Some(files),
    })
}

pub async fn read_log(user: AuthUser, pool: web::Data<DBPool>, query: web::Query<ReadLogQuery>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_LOG_READ) {
        return err.error_response();
    }
    // Reads are bounded to the log roots with the same managed-root
    // machinery as the file API: absolute paths only, no `..`, no symlink
    // components, opened through the fd the validator produced.
    let bound = match path_bounds::resolve_existing_under(&query.path, &log_roots()) {
        Ok(bound) => bound,
        Err(err) => {
            return HttpResponse::Forbidden().json(ResponseStructureError {
                success: false,
                code: 403,
                message: err.message().to_string(),
            })
        }
    };
    let std_file = match path_bounds::open_bound_file(&bound) {
        Ok(file) => file,
        Err(err) => {
            let (mut response, status) = if err.is_escape() {
                (
                    HttpResponse::Forbidden(),
                    actix_web::http::StatusCode::FORBIDDEN,
                )
            } else {
                (
                    HttpResponse::InternalServerError(),
                    actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                )
            };
            return response.json(ResponseStructureError {
                success: false,
                code: status.as_u16() as i16,
                message: err.message().to_string(),
            });
        }
    };

    let max_lines = query.lines.unwrap_or(100);
    let keyword = query.keyword.as_deref();

    let file = fs::File::from_std(std_file);
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut all_lines = Vec::new();

    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                all_lines.push(line);
                if all_lines.len() >= MAX_LOG_LINES {
                    break;
                }
            }
            Ok(None) => break,
            Err(_) => break,
        }
    }

    // Filter by keyword if provided — applied before tail slicing
    // so total reflects the full filtered set (capped at MAX_LOG_LINES).
    let filtered: Vec<String> = if let Some(kw) = keyword {
        all_lines.into_iter().filter(|l| l.contains(kw)).collect()
    } else {
        all_lines
    };

    let total = filtered.len();

    // Take last N lines
    let lines_result: Vec<String> = if filtered.len() > max_lines {
        filtered[filtered.len() - max_lines..].to_vec()
    } else {
        filtered
    };

    HttpResponse::Ok().json(ResponseStructure {
        success: true,
        code: 200,
        message: String::from("success"),
        data: Some(serde_json::json!({
            "path": query.path,
            "lines": lines_result,
            "total": total,
        })),
    })
}
