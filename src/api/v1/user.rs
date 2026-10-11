/*
 * @Descripttion:
 * @version:
 * @Author: Wynters
 * @Date: 2024-05-10 16:11:29
 * @LastEditTime: 2024-05-29 16:17:40
 * @FilePath: \RustPanel\src\api\v1\user.rs
 */

use actix_web::{web, HttpResponse, ResponseError};
use diesel::prelude::*;
use serde::Deserialize;

use crate::api::auth::AuthUser;
use crate::api::{ResponseStructure, ResponseStructureError};
use crate::models::structure::schema::rp_users;
use crate::service::db::DBPool;
use crate::service::password::{self, ALGO_ARGON2ID};
use crate::service::sessions;

pub async fn get_me(user: AuthUser, pool: web::Data<DBPool>) -> HttpResponse {
    let id = user.id;
    let (roles, permissions) = match web::block(move || {
        let mut conn = pool.get().map_err(|_| {
            crate::errors::common::CommonError::ServiceUnavailable(
                "Authorization database unavailable".into(),
            )
        })?;
        crate::service::rbac::capabilities(&mut conn, id)
    })
    .await
    {
        Ok(Ok(capabilities)) => capabilities,
        Ok(Err(error)) => return error.error_response(),
        Err(_) => return ResponseStructureError::internal("Authorization service unavailable"),
    };
    HttpResponse::Ok().json(serde_json::json!({
        "id": user.id,
        "username": user.username,
        "authority": user.authority,
        "created_at": user.created_at,
        "must_change_password": user.must_change_password,
        "roles": roles,
        "permissions": permissions,
    }))
}

#[derive(Deserialize)]
pub struct ChangePasswordBody {
    pub current_password: String,
    pub new_password: String,
}

pub async fn change_password(
    user: AuthUser,
    pool: web::Data<DBPool>,
    body: web::Json<ChangePasswordBody>,
) -> HttpResponse {
    if body.new_password.len() < 8 {
        return ResponseStructureError::bad("New password must be at least 8 characters");
    }
    let user_id = user.id;
    let token_version = user.token_version;
    let jti = user.jti.clone();
    let current = body.current_password.clone();
    let new_password = body.new_password.clone();
    match web::block(move || {
        let mut conn = pool.get().map_err(|_| {
            crate::errors::common::CommonError::ServiceUnavailable("Database unavailable".into())
        })?;
        conn.immediate_transaction::<_, crate::errors::common::CommonError, _>(|conn| {
            let row: crate::models::structure::rp_users::Users =
                rp_users::table.find(user_id).first(conn).map_err(|_| {
                    crate::errors::common::CommonError::Unauthorized("Invalid credentials".into())
                })?;
            if row.status != 0 || row.token_version != token_version {
                return Err(crate::errors::common::CommonError::Unauthorized(
                    "Invalid credentials".into(),
                ));
            }
            sessions::validate_session(conn, &jti, user_id)?;
            let (ok, _) =
                password::verify_stored(&row.password, &row.salt, &row.password_algo, &current)
                    .map_err(|_| {
                        crate::errors::common::CommonError::InternalServerError(
                            "Password verify failed".into(),
                        )
                    })?;
            if !ok {
                return Err(crate::errors::common::CommonError::BadRequest(
                    "Current password is incorrect".into(),
                ));
            }
            let hash = password::hash_argon2id(&new_password).map_err(|_| {
                crate::errors::common::CommonError::InternalServerError(
                    "Failed to hash password".into(),
                )
            })?;
            diesel::update(rp_users::table.find(user_id))
                .set((
                    rp_users::password.eq(&hash),
                    rp_users::salt.eq(""),
                    rp_users::password_algo.eq(ALGO_ARGON2ID),
                    rp_users::must_change_password.eq(0),
                ))
                .execute(conn)
                .map_err(|e| {
                    crate::errors::common::CommonError::InternalServerError(e.to_string())
                })?;
            // Revocation is part of the password change, not best-effort
            // follow-up: if it fails the transaction rolls back so the old
            // password and its tokens stay a consistent pair.
            sessions::bump_token_version(conn, user_id)
                .map_err(|err| crate::errors::common::CommonError::InternalServerError(err))?;
            sessions::revoke_all_for_user(conn, user_id)
                .map_err(|err| crate::errors::common::CommonError::InternalServerError(err))?;
            Ok(())
        })
    })
    .await
    {
        Ok(Ok(())) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("password updated; please sign in again"),
            data: Some(serde_json::json!({"ok": true})),
        }),
        Ok(Err(err)) => err.error_response(),
        Err(_) => ResponseStructureError::internal("Password change unavailable"),
    }
}

pub async fn get_menus(user: AuthUser, pool: web::Data<DBPool>) -> HttpResponse {
    let id = user.id;
    let (roles, permissions) = match web::block(move || {
        let mut conn = pool.get().map_err(|_| {
            crate::errors::common::CommonError::ServiceUnavailable(
                "Authorization database unavailable".into(),
            )
        })?;
        crate::service::rbac::capabilities(&mut conn, id)
    })
    .await
    {
        Ok(Ok(capabilities)) => capabilities,
        Ok(Err(error)) => return error.error_response(),
        Err(_) => return ResponseStructureError::internal("Authorization service unavailable"),
    };
    let items = [
        ("Dashboard", "/home", "monitor:read", "⌂"),
        ("Web Server", "/webserver", "site:read", "🌐"),
        ("Files", "/file/list", "file:read", "📁"),
        ("Docker", "/docker/containers", "docker:read", "🐳"),
        ("System Info", "/system/info", "system:read", "ℹ"),
        ("Users", "/admin/users", "user:admin", "👥"),
        ("Processes", "/process", "process:read", "⚡"),
        ("Services", "/services", "service:read", "🔧"),
        ("Logs", "/logs", "log:read", "📋"),
        ("Monitor", "/monitor", "monitor:read", "📊"),
        ("Firewall", "/firewall", "firewall:read", "🛡"),
        ("Software", "/installer", "app:read", "📦"),
    ];
    let menus: Vec<_> = items.iter().enumerate().filter(|(_, item)| permissions.iter().any(|p| p == item.2))
        .map(|(index, (name, path, _, icon))| serde_json::json!({"id":index+1,"name":name,"path":path,"key":path,"icon":icon})).collect();
    HttpResponse::Ok().json(serde_json::json!({"success":true,"data":{
        "info":{"id":user.id,"username":user.username}, "roles":roles,"access":permissions,"menus":menus
    }}))
}
