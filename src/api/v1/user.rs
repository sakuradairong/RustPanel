/*
 * @Descripttion:
 * @version:
 * @Author: Wynters
 * @Date: 2024-05-10 16:11:29
 * @LastEditTime: 2024-05-29 16:17:40
 * @FilePath: \RustPanel\src\api\v1\user.rs
 */

use actix_web::{web, HttpResponse, ResponseError};
use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use serde::Deserialize;

use crate::api::auth::AuthUser;
use crate::api::{ResponseStructure, ResponseStructureError};
use crate::models::structure::schema::rp_users;
use crate::service::db::DBPool;
use crate::service::password::{self, ALGO_ARGON2ID};
use crate::service::sessions;

pub async fn get_me(user: AuthUser) -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({
        "id": user.id,
        "username": user.username,
        "authority": user.authority,
        "created_at": user.created_at,
        "must_change_password": user.must_change_password,
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
    let current = body.current_password.clone();
    let new_password = body.new_password.clone();
    match web::block(move || {
        let mut conn = pool.get().map_err(|_| {
            crate::errors::common::CommonError::ServiceUnavailable(
                "Database unavailable".into(),
            )
        })?;
        let row: crate::models::structure::rp_users::Users = rp_users::table
            .find(user_id)
            .first(&mut *conn)
            .map_err(|_| {
                crate::errors::common::CommonError::Unauthorized("Invalid credentials".into())
            })?;
        let (ok, _) = password::verify_stored(
            &row.password,
            &row.salt,
            &row.password_algo,
            &current,
        )
        .map_err(|_| {
            crate::errors::common::CommonError::InternalServerError("Password verify failed".into())
        })?;
        if !ok {
            return Err(crate::errors::common::CommonError::Unauthorized(
                "Current password is incorrect".into(),
            ));
        }
        let hash = password::hash_argon2id(&new_password).map_err(|_| {
            crate::errors::common::CommonError::InternalServerError("Failed to hash password".into())
        })?;
        conn.batch_execute("BEGIN IMMEDIATE").map_err(|e| {
            crate::errors::common::CommonError::InternalServerError(e.to_string())
        })?;
        let committed = (|| -> Result<(), crate::errors::common::CommonError> {
            diesel::update(rp_users::table.find(user_id))
                .set((
                    rp_users::password.eq(&hash),
                    rp_users::salt.eq(""),
                    rp_users::password_algo.eq(ALGO_ARGON2ID),
                    rp_users::must_change_password.eq(0),
                ))
                .execute(&mut *conn)
                .map_err(|e| {
                    crate::errors::common::CommonError::InternalServerError(e.to_string())
                })?;
            // Revocation is part of the password change, not best-effort
            // follow-up: if it fails the transaction rolls back so the old
            // password and its tokens stay a consistent pair.
            sessions::bump_token_version(&mut conn, user_id).map_err(|err| {
                crate::errors::common::CommonError::InternalServerError(err)
            })?;
            sessions::revoke_all_for_user(&mut conn, user_id).map_err(|err| {
                crate::errors::common::CommonError::InternalServerError(err)
            })?;
            Ok(())
        })();
        match committed {
            Ok(()) => {
                conn.batch_execute("COMMIT").map_err(|e| {
                    crate::errors::common::CommonError::InternalServerError(e.to_string())
                })?;
                Ok(())
            }
            Err(err) => {
                let _ = conn.batch_execute("ROLLBACK");
                Err(err)
            }
        }
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

pub async fn get_menus(_: AuthUser) -> HttpResponse {
    let menus = serde_json::json!([{"id":1,"name":"首页","path":"/home","icon":"⌂","key":"home","sort":1},{"id":2,"name":"网站管理","path":"/webserver","icon":"🌐","key":"webserver","sort":2},{"id":3,"name":"文件管理","path":"/file/list","icon":"📁","key":"file.list","sort":3},{"id":4,"name":"Docker","path":"/docker/containers","icon":"🐳","key":"docker","sort":4},{"id":5,"name":"系统信息","path":"/system/info","icon":"ℹ","key":"system.info","sort":5},{"id":6,"name":"用户管理","path":"/admin/users","icon":"👥","key":"admin.users","sort":6},{"id":7,"name":"进程管理","path":"/process","icon":"⚡","key":"process","sort":7},{"id":8,"name":"服务管理","path":"/services","icon":"🔧","key":"services","sort":8},{"id":9,"name":"日志查看","path":"/logs","icon":"📋","key":"logs","sort":9},{"id":10,"name":"实时监控","path":"/monitor","icon":"📊","key":"monitor","sort":10},{"id":11,"name":"防火墙","path":"/firewall","icon":"🛡","key":"firewall","sort":11}]);

    HttpResponse::Ok().json(serde_json::json!({"data":{"info":{"id":1,"username":"admin","nickname":"Xin Admin","avatar":"https://api.iconify.design/logos:github-copilot.svg?color=#888888","sex":"0","email":"111@qq.com","mobile":"15966666666","status":"1","group_id":1,"create_time":"2022-02-26 19:55:29","update_time":"2023-10-19 14:34:00"},"access":["admin.list", "admin.group", "home", "system","file.list","web.list", "system.dict", "admin.rule", "admin.list.list", "admin.list.add", "admin.list.edit", "admin.group.list", "admin.group.add", "admin.group.edit", "admin.group.rule", "admin.group.ruleEdit", "system.dict.list", "data.descriptions", "system.dict.add", "admin.rule.list", "admin.rule.add", "admin.rule.edit", "system.dict.item.list", "system.dict.item.add", "system.dict.item.edit", "data.list", "data.checkcard", "admin.group.delete", "system.dict.delete", "system.dict.edit", "system.dict.item.delete", "admin.list.delete", "admin.rule.delete", "user.list", "online.table", "user", "system.info", "online", "xinadmin", "admin", "data", "system.setting", "system.setting.querySettingGroup", "system.setting.addGroup", "system.setting.querySettingPid", "online.table.list", "online.table.edit", "online.table.delete", "online.table.devise", "online.table.saveData", "online.table.getData", "online.table.crud", "system.setting.list", "system.setting.add", "system.setting.edit", "system.setting.delete", "content.article.add", "content.article.edit", "content.article.delete", "content.article.list", "user.rule", "user.group", "user.list", "user.edit", "user.add", "user.delete", "user.group.add", "user.group.list", "user.group.edit", "user.group.delete", "user.group.rule", "user.group.ruleEdit", "user.money_log", "user.money_log.list", "user.money_log.add", "online.table.add", "user.money_log.delete", "data.form", "admin.list.updatePwd"],"menus":menus,"success":true,"status":200,"msg":"ok","showType":0}}))
}
