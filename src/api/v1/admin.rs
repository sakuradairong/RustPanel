/*
 * @Descripttion:
 * @version:
 * @Author: Wynters
 * @Date: 2024-05-18 18:00:00
 * @LastEditTime: 2024-05-18 18:00:00
 * @FilePath: \RustPanel\src\api\v1\admin.rs
 */

use actix_web::{web, HttpResponse};
use chrono::Utc;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

use crate::api::auth::AuthUser;
use crate::api::{ResponseStructure, ResponseStructureError};
use crate::common::fun::{generate_random_string, sha1_salt};
use crate::errors::common::CommonError;
use crate::models::structure::rp_users::Users;
use crate::models::structure::schema::rp_users;
use crate::service::db::DBPool;

#[derive(Deserialize)]
pub struct CreateUserBody {
    pub username: String,
    pub password: Option<String>,
    pub authority: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateUserPath {
    pub id: i32,
}

#[derive(Deserialize)]
pub struct UpdateUserBody {
    pub username: Option<String>,
    pub password: Option<String>,
    pub authority: Option<String>,
    pub status: Option<i32>,
}

#[derive(Deserialize)]
pub struct DeleteUserPath {
    pub id: i32,
}

#[derive(Deserialize)]
pub struct ResetPasswordPath {
    pub id: i32,
}

#[derive(Serialize)]
struct UserResponse {
    pub id: i32,
    pub username: String,
    pub authority: String,
    pub status: i32,
    pub created_at: chrono::NaiveDateTime,
}

#[derive(Serialize)]
struct CreateUserResponse {
    pub id: i32,
    pub username: String,
    pub authority: String,
    pub initial_password: String,
}

#[derive(Serialize)]
struct ResetPasswordResponse {
    pub id: i32,
    pub username: String,
    pub new_password: String,
}

fn validate_authority(authority: &str) -> bool {
    authority == "admin" || authority == "user"
}

fn require_admin(user: &AuthUser) -> Result<(), HttpResponse> {
    if user.authority != "admin" {
        return Err(HttpResponse::Forbidden().json(ResponseStructureError {
            success: false,
            code: 403,
            message: String::from("Admin privileges required"),
        }));
    }
    Ok(())
}


fn map_diesel_error(err: diesel::result::Error) -> HttpResponse {
    let common_err = CommonError::from(err);
    match common_err {
        CommonError::BadRequest(ref msg) => HttpResponse::BadRequest().json(ResponseStructureError {
            success: false,
            code: 400,
            message: msg.clone(),
        }),
        _ => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: common_err.to_string(),
        }),
    }
}

pub async fn list_users(user: AuthUser, pool: web::Data<DBPool>) -> HttpResponse {
    if let Err(resp) = require_admin(&user) { return resp; }
    let mut conn = match pool.get() {
        Ok(c) => c,
        Err(_) => {
            return HttpResponse::InternalServerError().json(ResponseStructureError {
                success: false,
                code: 500,
                message: String::from("Failed to get database connection"),
            });
        }
    };

    match rp_users::table
        .select((
            rp_users::id,
            rp_users::username,
            rp_users::authority,
            rp_users::status,
            rp_users::created_at,
        ))
        .load::<(i32, String, String, i32, chrono::NaiveDateTime)>(&mut *conn)
    {
        Ok(rows) => {
            let users: Vec<UserResponse> = rows
                .into_iter()
                .map(
                    |(id, username, authority, status, created_at)| UserResponse {
                        id,
                        username,
                        authority,
                        status,
                        created_at,
                    },
                )
                .collect();
            HttpResponse::Ok().json(ResponseStructure {
                success: true,
                code: 200,
                message: String::from("success"),
                data: Some(users),
            })
        }
        Err(err) => map_diesel_error(err),
    }
}

pub async fn create_user(user: AuthUser, pool: web::Data<DBPool>,
body: web::Json<CreateUserBody>,) -> HttpResponse {
    if let Err(resp) = require_admin(&user) { return resp; }
    let mut conn = match pool.get() {
        Ok(c) => c,
        Err(_) => {
            return HttpResponse::InternalServerError().json(ResponseStructureError {
                success: false,
                code: 500,
                message: String::from("Failed to get database connection"),
            });
        }
    };

    let authority = body
        .authority
        .clone()
        .unwrap_or_else(|| String::from("user"));
    if !validate_authority(&authority) {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false,
            code: 400,
            message: String::from("Authority must be 'admin' or 'user'"),
        });
    }

    let password = body
        .password
        .clone()
        .unwrap_or_else(|| generate_random_string(12));
    let salt = generate_random_string(10);
    let hashed_password = sha1_salt(password.clone(), salt.clone());
    let now = Utc::now().naive_utc();

    if let Err(err) = diesel::insert_into(rp_users::table)
        .values((
            rp_users::username.eq(&body.username),
            rp_users::password.eq(&hashed_password),
            rp_users::salt.eq(&salt),
            rp_users::initial_password.eq(&password),
            rp_users::authority.eq(&authority),
            rp_users::error_count.eq(0),
            rp_users::status.eq(0),
            rp_users::created_at.eq(now),
            rp_users::updated_at.eq(now),
        ))
        .execute(&mut *conn)
    {
        return map_diesel_error(err);
    }

    // Query back the inserted user
    match rp_users::table
        .filter(rp_users::username.eq(&body.username))
        .first::<Users>(&mut *conn)
    {
        Ok(user) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some(CreateUserResponse {
                id: user.id,
                username: user.username,
                authority: user.authority,
                initial_password: user.initial_password,
            }),
        }),
        Err(err) => map_diesel_error(err),
    }
}

pub async fn update_user(user: AuthUser, pool: web::Data<DBPool>,
path: web::Path<UpdateUserPath>,
body: web::Json<UpdateUserBody>,) -> HttpResponse {
    if let Err(resp) = require_admin(&user) { return resp; }
    let mut conn = match pool.get() {
        Ok(c) => c,
        Err(_) => {
            return HttpResponse::InternalServerError().json(ResponseStructureError {
                success: false,
                code: 500,
                message: String::from("Failed to get database connection"),
            });
        }
    };

    let id = path.id;

    // Validate authority if provided
    if let Some(ref authority) = body.authority {
        if !validate_authority(authority) {
            return HttpResponse::BadRequest().json(ResponseStructureError {
                success: false,
                code: 400,
                message: String::from("Authority must be 'admin' or 'user'"),
            });
        }
    }

    // Read the current user first
    let mut user = match rp_users::table.find(id).first::<Users>(&mut *conn) {
        Ok(u) => u,
        Err(diesel::result::Error::NotFound) => {
            return HttpResponse::NotFound().json(ResponseStructureError {
                success: false,
                code: 404,
                message: String::from("User not found"),
            });
        }
        Err(err) => return map_diesel_error(err),
    };

    // Merge changes
    if let Some(ref username) = body.username {
        user.username = username.clone();
    }
    if let Some(ref password) = body.password {
        let salt = generate_random_string(10);
        user.password = sha1_salt(password.clone(), salt.clone());
        user.salt = salt;
    }
    if let Some(ref authority) = body.authority {
        user.authority = authority.clone();
    }
    if let Some(status) = body.status {
        user.status = status;
    }
    user.updated_at = Utc::now().naive_utc();

    match diesel::update(rp_users::table.find(id))
        .set((
            rp_users::username.eq(&user.username),
            rp_users::password.eq(&user.password),
            rp_users::salt.eq(&user.salt),
            rp_users::authority.eq(&user.authority),
            rp_users::status.eq(user.status),
            rp_users::updated_at.eq(user.updated_at),
        ))
        .execute(&mut *conn)
    {
        Ok(_) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some(UserResponse {
                id: user.id,
                username: user.username.clone(),
                authority: user.authority.clone(),
                status: user.status,
                created_at: user.created_at,
            }),
        }),
        Err(err) => map_diesel_error(err),
    }
}

pub async fn delete_user(user: AuthUser, pool: web::Data<DBPool>,
path: web::Path<DeleteUserPath>,) -> HttpResponse {
    if let Err(resp) = require_admin(&user) { return resp; }
    let mut conn = match pool.get() {
        Ok(c) => c,
        Err(_) => {
            return HttpResponse::InternalServerError().json(ResponseStructureError {
                success: false,
                code: 500,
                message: String::from("Failed to get database connection"),
            });
        }
    };

    match diesel::delete(rp_users::table.find(path.id)).execute(&mut *conn) {
        Ok(0) => HttpResponse::NotFound().json(ResponseStructureError {
            success: false,
            code: 404,
            message: String::from("User not found"),
        }),
        Ok(_) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("success"),
            data: None::<bool>,
        }),
        Err(err) => map_diesel_error(err),
    }
}

pub async fn reset_password(user: AuthUser, pool: web::Data<DBPool>,
path: web::Path<ResetPasswordPath>,) -> HttpResponse {
    if let Err(resp) = require_admin(&user) { return resp; }
    let mut conn = match pool.get() {
        Ok(c) => c,
        Err(_) => {
            return HttpResponse::InternalServerError().json(ResponseStructureError {
                success: false,
                code: 500,
                message: String::from("Failed to get database connection"),
            });
        }
    };

    let user = match rp_users::table.find(path.id).first::<Users>(&mut *conn) {
        Ok(u) => u,
        Err(diesel::result::Error::NotFound) => {
            return HttpResponse::NotFound().json(ResponseStructureError {
                success: false,
                code: 404,
                message: String::from("User not found"),
            });
        }
        Err(err) => return map_diesel_error(err),
    };

    let new_password = generate_random_string(12);
    let salt = generate_random_string(10);
    let hashed_password = sha1_salt(new_password.clone(), salt.clone());
    let now = Utc::now().naive_utc();

    match diesel::update(rp_users::table.find(path.id))
        .set((
            rp_users::password.eq(&hashed_password),
            rp_users::salt.eq(&salt),
            rp_users::initial_password.eq(&new_password),
            rp_users::updated_at.eq(now),
        ))
        .execute(&mut *conn)
    {
        Ok(_) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some(ResetPasswordResponse {
                id: user.id,
                username: user.username,
                new_password,
            }),
        }),
        Err(err) => map_diesel_error(err),
    }
}
