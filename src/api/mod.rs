/*
 * @Descripttion:
 * @version:
 * @Author: Wynters
 * @Date: 2024-05-09 18:54:56
 * @LastEditTime: 2024-05-12 19:55:52
 * @FilePath: \RustPanel\src\api\mod.rs
 */

pub mod auth;
pub mod route;
pub mod v1;
//pub mod middleware;

use actix_web::HttpResponse;
use serde::Serialize;
#[derive(Serialize)]
pub struct ResponseStructure<T> {
    pub success: bool,
    pub data: Option<T>,
    pub code: i16,
    pub message: String,
}
#[derive(Serialize)]
pub struct ResponseStructureError {
    pub success: bool,
    pub code: i16,
    pub message: String,
}

// ── Response helpers ──

impl<T: Serialize> ResponseStructure<T> {
    pub fn ok(data: T) -> HttpResponse {
        HttpResponse::Ok().json(Self { success: true, code: 200, message: String::from("success"), data: Some(data) })
    }
    pub fn ok_msg(msg: &str, data: T) -> HttpResponse {
        HttpResponse::Ok().json(Self { success: true, code: 200, message: msg.to_string(), data: Some(data) })
    }
}

impl ResponseStructureError {
    pub fn bad(msg: &str) -> HttpResponse {
        HttpResponse::BadRequest().json(Self { success: false, code: 400, message: msg.to_string() })
    }
    pub fn internal(msg: &str) -> HttpResponse {
        HttpResponse::InternalServerError().json(Self { success: false, code: 500, message: msg.to_string() })
    }
    pub fn not_found(msg: &str) -> HttpResponse {
        HttpResponse::NotFound().json(Self { success: false, code: 404, message: msg.to_string() })
    }
    pub fn forbidden(msg: &str) -> HttpResponse {
        HttpResponse::Forbidden().json(Self { success: false, code: 403, message: msg.to_string() })
    }
    pub fn unauthorized(msg: &str) -> HttpResponse {
        HttpResponse::Unauthorized().json(Self { success: false, code: 401, message: msg.to_string() })
    }
}


pub async fn index() -> HttpResponse {
    HttpResponse::Forbidden().body("403 Forbidden")
}
