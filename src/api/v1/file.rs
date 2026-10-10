use crate::api::auth::AuthUser;
use crate::api::{ResponseStructure, ResponseStructureError};
use crate::models::file::{self, SaveData};
use crate::service::db::DBPool;
use crate::service::path_bounds::{self, FileRootConfig, PathBoundError};
use crate::service::rbac::{self, PERM_FILE_READ, PERM_FILE_WRITE};

use actix_web::{web, HttpResponse, ResponseError};
use serde::Deserialize;
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
    HttpResponse::Ok().json(ResponseStructure {
        success: true,
        code: 200,
        message: String::from("success"),
        data: Some(file::get_files_and_dirs_list(
            bound.absolute.to_string_lossy().as_ref(),
        )),
    })
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
    match file::save_bounded(bound, data.into_inner().content) {
        Ok(_) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some("ok"),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: err.to_string(),
        }),
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
