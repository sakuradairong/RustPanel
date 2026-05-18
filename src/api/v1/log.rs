/*
 * @Descripttion:
 * @version:
 * @Author: Wynters
 * @Date: 2024-05-25 16:23:02
 * @FilePath: \RustPanel\src\api\v1\log.rs
 */

use crate::api::auth::AuthUser;
use crate::api::{ResponseStructure, ResponseStructureError};
use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::fs;

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

pub async fn list_logs(_: AuthUser) -> HttpResponse {
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
                        path: path.to_string_lossy().to_string(),
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

pub async fn read_log(_: AuthUser, query: web::Query<ReadLogQuery>) -> HttpResponse {
    // Security: reject paths containing ".." to prevent directory traversal
    if query.path.contains("..") {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false,
            code: 400,
            message: String::from("Path must not contain '..'"),
        });
    }

    let max_lines = query.lines.unwrap_or(100);
    let keyword = query.keyword.as_deref();

    match fs::File::open(&query.path).await {
        Ok(file) => {
            let reader = BufReader::new(file);
            let mut lines = reader.lines();
            let mut all_lines = Vec::new();

            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => all_lines.push(line),
                    Ok(None) => break,
                    Err(_) => break,
                }
            }

            // Filter by keyword if provided — applied before tail slicing
            // so total reflects the full filtered set
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
        Err(e) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: e.to_string(),
        }),
    }
}
