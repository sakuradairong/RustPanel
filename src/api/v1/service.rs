/*
 * @Descripttion: Systemd service management API handlers
 * @version:
 * @Author: Wynters
 * @Date: 2024-05-25 16:23:02
 * @LastEditTime: 2024-06-02 10:00:00
 * @FilePath: \RustPanel\src\api\v1\service.rs
 */

use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::api::auth::AuthUser;
use crate::api::{ResponseStructure, ResponseStructureError};

#[derive(Deserialize)]
pub struct ServiceNamePath {
    pub name: String,
}

#[derive(Serialize)]
pub struct ServiceInfo {
    pub name: String,
    pub load: String,
    pub active: String,
    pub sub: String,
    pub description: String,
}

fn is_linux() -> bool {
    cfg!(target_os = "linux")
}

pub async fn list_services(_: AuthUser) -> HttpResponse {
    if !is_linux() {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false,
            code: 400,
            message: String::from("Service management is only supported on Linux"),
        });
    }

    let output = Command::new("systemctl")
        .args([
            "list-units",
            "--type=service",
            "--no-pager",
            "--no-legend",
        ])
        .output()
        .await;

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let services: Vec<ServiceInfo> = stdout
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| {
                    let mut iter = line.split_whitespace();
                    ServiceInfo {
                        name: iter.next().unwrap_or("").to_string(),
                        load: iter.next().unwrap_or("").to_string(),
                        active: iter.next().unwrap_or("").to_string(),
                        sub: iter.next().unwrap_or("").to_string(),
                        description: iter
                            .collect::<Vec<&str>>()
                            .join(" "),
                    }
                })
                .collect();

            HttpResponse::Ok().json(ResponseStructure {
                success: true,
                code: 200,
                message: String::from("success"),
                data: Some(services),
            })
        }
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: err.to_string(),
        }),
    }
}

async fn run_systemctl_command(action: &str, name: &str) -> HttpResponse {
    let output = Command::new("systemctl")
        .arg(action)
        .arg(name)
        .output()
        .await;

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let success = output.status.success();

            if success {
                HttpResponse::Ok().json(ResponseStructure {
                    success: true,
                    code: 200,
                    message: String::from("success"),
                    data: Some(serde_json::json!({
                        "stdout": stdout,
                        "stderr": stderr,
                    })),
                })
            } else {
                HttpResponse::InternalServerError().json(ResponseStructureError {
                    success: false,
                    code: 500,
                    message: if stderr.is_empty() { stdout } else { stderr },
                })
            }
        }
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: err.to_string(),
        }),
    }
}

pub async fn start_service(_: AuthUser, path: web::Path<ServiceNamePath>) -> HttpResponse {
    if !is_linux() {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false,
            code: 400,
            message: String::from("Service management is only supported on Linux"),
        });
    }
    run_systemctl_command("start", &path.name).await
}

pub async fn stop_service(_: AuthUser, path: web::Path<ServiceNamePath>) -> HttpResponse {
    if !is_linux() {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false,
            code: 400,
            message: String::from("Service management is only supported on Linux"),
        });
    }
    run_systemctl_command("stop", &path.name).await
}

pub async fn restart_service(_: AuthUser, path: web::Path<ServiceNamePath>) -> HttpResponse {
    if !is_linux() {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false,
            code: 400,
            message: String::from("Service management is only supported on Linux"),
        });
    }
    run_systemctl_command("restart", &path.name).await
}

pub async fn get_service_status(_: AuthUser, path: web::Path<ServiceNamePath>) -> HttpResponse {
    if !is_linux() {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false,
            code: 400,
            message: String::from("Service management is only supported on Linux"),
        });
    }

    let output = Command::new("systemctl")
        .args([
            "status",
            &path.name,
            "--no-pager",
            "--lines=20",
        ])
        .output()
        .await;

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let success = output.status.success();

            if success {
                HttpResponse::Ok().json(ResponseStructure {
                    success: true,
                    code: 200,
                    message: String::from("success"),
                    data: Some(serde_json::json!({
                        "status": stdout,
                        "logs": stderr,
                    })),
                })
            } else {
                HttpResponse::InternalServerError().json(ResponseStructureError {
                    success: false,
                    code: 500,
                    message: if stderr.is_empty() { stdout } else { stderr },
                })
            }
        }
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: err.to_string(),
        }),
    }
}
