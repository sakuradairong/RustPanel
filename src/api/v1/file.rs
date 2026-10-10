use crate::api::auth::AuthUser;
use crate::api::{ResponseStructure, ResponseStructureError};
use crate::audit::{self, IntentSpec, Outcome};
use crate::models::file::{self, SaveData};
use crate::privileged::runner;
use crate::service::db::DBPool;
use crate::service::path_bounds::{self, FileRootConfig, PathBoundError};
use crate::service::rbac::{self, PERM_FILE_READ, PERM_FILE_WRITE};

use actix_web::{web, HttpResponse, ResponseError};
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;

#[derive(Deserialize)]
pub struct ListData {
    pub path: String,
    pub current: i32,
    #[serde(rename = "pageSize")]
    pub page_size: i32,
}

fn path_error_response(err: PathBoundError) -> HttpResponse {
    if err.is_escape() {
        ResponseStructureError::forbidden(err.message())
    } else {
        ResponseStructureError::internal(err.message())
    }
}

fn effective_roots(roots: &Option<web::Data<FileRootConfig>>) -> Vec<PathBuf> {
    roots
        .as_ref()
        .map(|cfg| cfg.roots.clone())
        .unwrap_or_else(path_bounds::managed_roots)
}

pub async fn list(
    user: AuthUser,
    pool: web::Data<DBPool>,
    roots: Option<web::Data<FileRootConfig>>,
    data: web::Query<ListData>,
) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_FILE_READ) {
        return err.error_response();
    }
    let bound = match path_bounds::resolve_existing_under(&data.path, &effective_roots(&roots)) {
        Ok(bound) => bound,
        Err(err) => return path_error_response(err),
    };
    match file::list_bounded(bound) {
        Ok(listing) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some(listing),
        }),
        Err(err) => path_error_response(err),
    }
}

pub async fn save(
    user: AuthUser,
    pool: web::Data<DBPool>,
    roots: Option<web::Data<FileRootConfig>>,
    data: web::Json<SaveData>,
) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_FILE_WRITE) {
        return err.error_response();
    }
    let bound = match path_bounds::resolve_for_write_under(&data.path, &effective_roots(&roots)) {
        Ok(bound) => bound,
        Err(err) => return path_error_response(err),
    };
    let path_display = bound.absolute.to_string_lossy().into_owned();
    let operation_id = runner::new_operation_id();
    let intent = match audit::begin_intent(
        pool.get_ref(),
        IntentSpec {
            operation_id: operation_id.clone(),
            actor_id: Some(user.id),
            request_id: None,
            action: "file.save".into(),
            resource: Some(path_display.clone()),
            params: json!({
                "path": path_display,
                "content": data.content,
            }),
            ip: None,
        },
    ) {
        Ok(intent) => intent,
        Err(err) => {
            return HttpResponse::ServiceUnavailable().json(ResponseStructureError {
                success: false,
                code: 503,
                message: err.message().to_string(),
            });
        }
    };
    match file::save_bounded(bound, data.into_inner().content) {
        Ok(_) => {
            if let Err(audit_err) = audit::complete_intent(
                pool.get_ref(),
                &intent,
                Outcome::Success,
                Some(0),
                json!({"ok": true}),
                None,
            ) {
                // The file is already written; surface the audit failure
                // instead of dropping it silently. Reconciliation marks the
                // intent aborted if the write truly failed.
                crate::log_error!(
                    target: "audit",
                    "audit completion failed for {}: {}",
                    intent.operation_id,
                    audit_err.message()
                );
            }
            HttpResponse::Ok().json(ResponseStructure {
                success: true,
                code: 200,
                message: String::from("success"),
                data: Some(serde_json::json!({
                    "ok": true,
                    "operation_id": operation_id,
                })),
            })
        }
        Err(err) => {
            if let Err(audit_err) = audit::complete_intent(
                pool.get_ref(),
                &intent,
                Outcome::Failure,
                None,
                json!({"error": err.to_string()}),
                None,
            ) {
                crate::log_error!(
                    target: "audit",
                    "audit completion failed for {}: {}",
                    intent.operation_id,
                    audit_err.message()
                );
            }
            HttpResponse::InternalServerError().json(ResponseStructureError {
                success: false,
                code: 500,
                message: err.to_string(),
            })
        }
    }
}

#[derive(Deserialize)]
pub struct ContentData {
    pub path: String,
}

pub async fn content(
    user: AuthUser,
    pool: web::Data<DBPool>,
    roots: Option<web::Data<FileRootConfig>>,
    data: web::Query<ContentData>,
) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_FILE_READ) {
        return err.error_response();
    }
    let bound = match path_bounds::resolve_existing_under(&data.path, &effective_roots(&roots)) {
        Ok(bound) => bound,
        Err(err) => return path_error_response(err),
    };
    match file::content_bounded(bound) {
        Ok((content, extension)) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("Success"),
            data: Some(serde_json::json!({"content":content,"ext":extension})),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: err.to_string(),
        }),
    }
}
