use crate::api::auth::AuthUser;
use crate::api::{ResponseStructure, ResponseStructureError};
use crate::models::file::{self, SaveData};
use crate::service::db::DBPool;
use crate::service::rbac::{self, PERM_FILE_READ, PERM_FILE_WRITE};

use actix_web::{web, HttpResponse, ResponseError};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct ListData {
    pub path: String,
    pub current: i32,
    #[serde(rename = "pageSize")]
    pub page_size: i32,
}

pub async fn list(user: AuthUser, pool: web::Data<DBPool>, data: web::Query<ListData>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_FILE_READ) {
        return err.error_response();
    }
    // Security: reject paths containing ".." to prevent directory traversal
    if data.path.contains("..") {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false,
            code: 400,
            message: String::from("Path must not contain '..'"),
        });
    }
    HttpResponse::Ok().json(ResponseStructure {
        success: true,
        code: 200,
        message: String::from("success"),
        data: Some(file::get_files_and_dirs_list(data.path.as_str())),
    })
}

pub async fn save(user: AuthUser, pool: web::Data<DBPool>, data: web::Json<SaveData>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_FILE_WRITE) {
        return err.error_response();
    }
    // Security: reject paths containing ".." to prevent directory traversal
    if data.path.contains("..") {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false,
            code: 400,
            message: String::from("Path must not contain '..'"),
        });
    }
    match file::save(data.into_inner()) {
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
    data: web::Query<ContentData>,
) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_FILE_READ) {
        return err.error_response();
    }
    // Security: reject paths containing ".." to prevent directory traversal
    if data.path.contains("..") {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false,
            code: 400,
            message: String::from("Path must not contain '..'"),
        });
    }
    match file::content(data.path.to_string()) {
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
