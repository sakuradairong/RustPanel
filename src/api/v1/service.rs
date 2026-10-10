/*
 * @Descripttion: Systemd service management API handlers
 * @version:
 * @Author: Wynters
 * @Date: 2024-05-25 16:23:02
 * @LastEditTime: 2024-06-02 10:00:00
 * @FilePath: \RustPanel\src\api\v1\service.rs
 */

use actix_web::{web, HttpResponse, ResponseError};
use serde::{Deserialize, Serialize};

use crate::api::auth::AuthUser;
use crate::api::{ResponseStructure, ResponseStructureError};
use crate::privileged::{self, Action, AuditedError, SystemctlVerb};
use crate::service::db::DBPool;
use crate::service::rbac::{self, PERM_SERVICE_WRITE};

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

fn runner_error_response(err: privileged::RunnerError) -> HttpResponse {
    if err.is_rejected() {
        ResponseStructureError::forbidden(&err.message())
    } else {
        HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: err.message(),
        })
    }
}

fn audited_error_response(err: AuditedError) -> HttpResponse {
    if err.is_unavailable() {
        HttpResponse::ServiceUnavailable().json(ResponseStructureError {
            success: false,
            code: 503,
            message: err.message(),
        })
    } else if err.is_rejected() {
        ResponseStructureError::forbidden(&err.message())
    } else {
        HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: err.message(),
        })
    }
}

pub async fn list_services(user: AuthUser, _pool: web::Data<DBPool>) -> HttpResponse {
    if !is_linux() {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false,
            code: 400,
            message: String::from("Service management is only supported on Linux"),
        });
    }

    match privileged::execute(privileged::runner::request_for(
        Some(user.id),
        Action::Systemctl {
            verb: SystemctlVerb::ListUnits,
            unit: None,
        },
    ))
    .await
    {
        Ok(result) => {
            let services: Vec<ServiceInfo> = result
                .stdout
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| {
                    let mut iter = line.split_whitespace();
                    ServiceInfo {
                        name: iter.next().unwrap_or("").to_string(),
                        load: iter.next().unwrap_or("").to_string(),
                        active: iter.next().unwrap_or("").to_string(),
                        sub: iter.next().unwrap_or("").to_string(),
                        description: iter.collect::<Vec<&str>>().join(" "),
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
        Err(err) => runner_error_response(err),
    }
}

async fn run_systemctl(
    user: &AuthUser,
    pool: &DBPool,
    verb: SystemctlVerb,
    name: &str,
) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool, user, PERM_SERVICE_WRITE) {
        return err.error_response();
    }
    match privileged::execute_audited(
        pool,
        Some(user.id),
        Action::Systemctl {
            verb,
            unit: Some(name.to_string()),
        },
    )
    .await
    {
        Ok(result) if result.exit_code == Some(0) || verb == SystemctlVerb::Status => {
            HttpResponse::Ok().json(ResponseStructure {
                success: true,
                code: 200,
                message: String::from("success"),
                data: Some(serde_json::json!({
                    "stdout": result.stdout,
                    "stderr": result.stderr,
                    "operation_id": result.operation_id,
                    "duration_ms": result.duration_ms,
                })),
            })
        }
        Ok(result) => {
            let message = if result.stderr.is_empty() {
                result.stdout
            } else {
                result.stderr
            };
            HttpResponse::InternalServerError().json(ResponseStructureError {
                success: false,
                code: 500,
                message,
            })
        }
        Err(err) => audited_error_response(err),
    }
}

pub async fn start_service(
    user: AuthUser,
    pool: web::Data<DBPool>,
    path: web::Path<ServiceNamePath>,
) -> HttpResponse {
    if !is_linux() {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false,
            code: 400,
            message: String::from("Service management is only supported on Linux"),
        });
    }
    run_systemctl(&user, pool.get_ref(), SystemctlVerb::Start, &path.name).await
}

pub async fn stop_service(
    user: AuthUser,
    pool: web::Data<DBPool>,
    path: web::Path<ServiceNamePath>,
) -> HttpResponse {
    if !is_linux() {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false,
            code: 400,
            message: String::from("Service management is only supported on Linux"),
        });
    }
    run_systemctl(&user, pool.get_ref(), SystemctlVerb::Stop, &path.name).await
}

pub async fn restart_service(
    user: AuthUser,
    pool: web::Data<DBPool>,
    path: web::Path<ServiceNamePath>,
) -> HttpResponse {
    if !is_linux() {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false,
            code: 400,
            message: String::from("Service management is only supported on Linux"),
        });
    }
    run_systemctl(&user, pool.get_ref(), SystemctlVerb::Restart, &path.name).await
}

pub async fn get_service_status(
    user: AuthUser,
    pool: web::Data<DBPool>,
    path: web::Path<ServiceNamePath>,
) -> HttpResponse {
    if !is_linux() {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false,
            code: 400,
            message: String::from("Service management is only supported on Linux"),
        });
    }
    // Status is read-ish; still validate unit via runner and require write perm for M0
    // consistency with other mutating service routes until a service:read perm exists.
    run_systemctl(&user, pool.get_ref(), SystemctlVerb::Status, &path.name).await
}
