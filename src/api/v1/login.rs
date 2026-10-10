/*
 * @Descripttion:
 * @version:
 * @Author: Wynters
 * @Date: 2024-05-10 01:54:06
 * @LastEditTime: 2024-05-20 18:53:14
 * @FilePath: \RustPanel\src\api\v1\login.rs
 */

use actix_web::{web, Error, HttpResponse};
use rust_i18n::t;
use std::{
    collections::HashMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{
    api::{
        auth::{generate_jwt, AuthUser},
        ResponseStructure, ResponseStructureError,
    },
    common::fun::sm4_decrypt_login,
    errors::common::CommonError,
    models::user::find_user_update_error_count,
    service::{
        db::DBPool,
        global::CONF,
        password::{self, ALGO_ARGON2ID},
    },
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct LoginAuthData {
    pub username: String,
    pub password: String,
    pub timestamp: u64,
}
#[derive(Serialize)]
struct LoginSuccessData {
    token: String,
    user: serde_json::Value,
}

pub async fn sign(
    pool: web::Data<DBPool>,
    auth_data: web::Json<LoginAuthData>,
    security_dir: web::Path<String>,
) -> Result<HttpResponse, Error> {
    if CONF
        .app
        .security_dir
        .as_str()
        .eq(&security_dir.into_inner())
    {
        let system_time = UNIX_EPOCH + Duration::from_millis(auth_data.timestamp);
        let now = SystemTime::now();
        if let Ok(duration) = now.duration_since(system_time) {
            if duration > Duration::from_secs(60) {
                return Ok(
                    HttpResponse::InternalServerError().json(ResponseStructureError {
                        success: false,
                        code: 101,
                        message: String::from("Invalid timestamp.-1"),
                    }),
                );
            }
        } /*else {
              return Ok(HttpResponse::InternalServerError().json(ResponseStructureError {
                  success: false,
                  code: 101,
                  message: String::from("Invalid timestamp.-2"),
              }));
          }*/

        let user_data: LoginAuthData = LoginAuthData {
            username: sm4_decrypt_login(
                auth_data.username.to_string(),
                CONF.app.security_dir.to_string(),
                auth_data.timestamp.to_string(),
            ),
            password: sm4_decrypt_login(
                auth_data.password.to_string(),
                CONF.app.security_dir.to_string(),
                auth_data.timestamp.to_string(),
            ),
            timestamp: 0,
        };

        let db_user = web::block(move || query(user_data, pool)).await??;
        let must_change_password = db_user.must_change_password != 0;
        let user: AuthUser = db_user.into();

        //   Identity::login(&req.extensions(), serde_json::to_string(&user).unwrap()).unwrap();
        //返回Cookies
        let user_json = match serde_json::to_string(&user) {
            Ok(s) => s,
            Err(_) => {
                return Ok(
                    HttpResponse::InternalServerError().json(ResponseStructureError {
                        success: false,
                        code: 500,
                        message: String::from("serialize user error"),
                    }),
                )
            }
        };

        let token_val = match generate_jwt(&user_json) {
            Ok(t) => t,
            Err(_) => {
                return Ok(
                    HttpResponse::InternalServerError().json(ResponseStructureError {
                        success: false,
                        code: 500,
                        message: String::from("generate token error"),
                    }),
                )
            }
        };

        return Ok(HttpResponse::Ok()
            .cookie(
                actix_web::cookie::Cookie::build(
                    "token",
                    token_val.clone(),
                )
                .finish(),
            )
            .json(ResponseStructure {
                success: true,
                code: 200,
                message: String::from("success"),
                data: Some(LoginSuccessData {
                    token: token_val,
                    user: serde_json::json!({
                        "id": user.id,
                        "username": user.username,
                        "authority": user.authority,
                        "must_change_password": must_change_password,
                    })
                }),
              
            }));

        //return Ok(HttpResponse::Ok().body("Login Success"))
    }
    Ok(HttpResponse::NotFound().finish())
}

pub async fn get_security_dir(
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse, Error> {
    // 尝试从查询参数中获取 "key"，如果 "key" 参数不存在，返回 400 错误
    let security_dir = match query.get("v") {
        Some(key) => key,
        None => {
            return Ok(
                HttpResponse::InternalServerError().json(ResponseStructureError {
                    success: false,
                    code: 101,
                    message: String::from("Security directory does not match the query parameter"),
                }),
            )
        }
    };

    if security_dir == &CONF.app.security_dir {
        // 如果匹配，返回成功的响应
        Ok(HttpResponse::Ok().json(ResponseStructureError {
            success: true,
            code: 200,
            message: String::from("success"),
        }))
    } else {
        // 如果不匹配，返回 404 错误，表示未找到
        Ok(
            HttpResponse::InternalServerError().json(ResponseStructureError {
                success: false,
                code: 101,
                message: String::from("Security directory does not match the query parameter"),
            }),
        )
    }
}

fn query(
    auth_data: LoginAuthData,
    pool: web::Data<DBPool>,
) -> Result<crate::models::structure::rp_users::Users, CommonError> {
    use crate::models::structure::{
        rp_users::Users,
        schema::rp_users::dsl::{rp_users, status, username},
    };
    use diesel::{prelude::*, ExpressionMethods, QueryDsl};

    let mut conn = match pool.get() {
        Ok(c) => c,
        Err(_) => {
            return Err(CommonError::InternalServerError(String::from(
                "Failed to get a connection from the pool",
            ))
            .into())
        }
    };
    let mut items = rp_users
        .filter(username.eq(&auth_data.username))
        .load::<Users>(&mut conn)?;

    if let Some(mut user) = items.pop() {
        if user.status != 0 {
            return Err(CommonError::Unauthorized("Invalid credentials".into()));
        }
        if user.error_count >= 5 {
            diesel::update(rp_users.find(user.id))
                .set(status.eq(-1))
                .execute(&mut conn)?;
            return Err(
                CommonError::BadRequest(String::from(t!("auth.login.username_band"))).into(),
            );
        }
        let (matching, should_rehash) = password::verify_stored(
            &user.password,
            &user.salt,
            &user.password_algo,
            &auth_data.password,
        )
        .map_err(|_| {
            CommonError::BadRequest(String::from(
                t!("auth.login.password_error",count=> 5 - (user.error_count+1)),
            ))
        })?;
        if matching {
            let _ = find_user_update_error_count(user.id, 0, &mut conn)?;
            if should_rehash {
                let new_hash = password::hash_argon2id(&auth_data.password).map_err(|_| {
                    CommonError::InternalServerError("Failed to upgrade password hash".into())
                })?;
                diesel::update(rp_users.find(user.id))
                    .set((
                        crate::models::structure::schema::rp_users::password.eq(&new_hash),
                        crate::models::structure::schema::rp_users::salt.eq(""),
                        crate::models::structure::schema::rp_users::password_algo.eq(ALGO_ARGON2ID),
                    ))
                    .execute(&mut conn)?;
                user.password = new_hash;
                user.salt.clear();
                user.password_algo = ALGO_ARGON2ID.to_owned();
            }
            return Ok(user);
        }
        let _ = find_user_update_error_count(user.id, user.error_count + 1, &mut conn)?;
        return Err(CommonError::BadRequest(String::from(
            t!("auth.login.password_error",count=> 5 - (user.error_count+1)),
        ))
        .into());
    }
    Err(CommonError::BadRequest(String::from(t!("auth.login.username_error"))).into())
}

#[cfg(test)]
mod tests {
    use super::{query, LoginAuthData};
    use crate::{
        common::fun::sha1_salt,
        errors::common::CommonError,
        repository::migrations,
        service::{db::DBPool, password::ALGO_SHA1},
    };
    use actix_web::web;
    use diesel::{connection::SimpleConnection, r2d2::ConnectionManager, SqliteConnection};

    fn pool() -> DBPool {
        let file = tempfile::NamedTempFile::new().unwrap();
        // Leak the temp path for the pool lifetime in this unit test process.
        let path = file.into_temp_path();
        let path_str = path.to_str().unwrap().to_owned();
        std::mem::forget(path);
        DBPool::builder()
            .max_size(1)
            .build(ConnectionManager::<SqliteConnection>::new(path_str))
            .unwrap()
    }

    #[test]
    fn disabled_account_cannot_log_in_with_the_correct_password() {
        let pool = pool();
        migrations::migrate(&pool).unwrap();
        let password_hash = sha1_salt("test-password".into(), "test-salt".into());
        pool.get()
            .unwrap()
            .batch_execute(&format!(
                "INSERT INTO rp_users (
                    username, password, salt, authority, error_count, status,
                    created_at, updated_at, password_algo, must_change_password
                ) VALUES (
                    'test-user', '{password_hash}', 'test-salt', 'user', 0, -1,
                    '2026-10-10 00:00:00', '2026-10-10 00:00:00', '{ALGO_SHA1}', 0
                );"
            ))
            .unwrap();
        let credentials = || LoginAuthData {
            username: "test-user".into(),
            password: "test-password".into(),
            timestamp: 0,
        };
        assert!(matches!(
            query(credentials(), web::Data::new(pool.clone())),
            Err(CommonError::Unauthorized(_))
        ));
        pool.get()
            .unwrap()
            .batch_execute("UPDATE rp_users SET status=0")
            .unwrap();
        assert!(query(credentials(), web::Data::new(pool)).is_ok());
    }

    #[test]
    fn successful_sha1_login_rehashes_to_argon2id() {
        let pool = pool();
        migrations::migrate(&pool).unwrap();
        let password_hash = sha1_salt("migrate-me".into(), "old-salt".into());
        pool.get()
            .unwrap()
            .batch_execute(&format!(
                "INSERT INTO rp_users (
                    username, password, salt, authority, error_count, status,
                    created_at, updated_at, password_algo, must_change_password
                ) VALUES (
                    'legacy-user', '{password_hash}', 'old-salt', 'user', 0, 0,
                    '2026-10-10 00:00:00', '2026-10-10 00:00:00', '{ALGO_SHA1}', 0
                );"
            ))
            .unwrap();
        let user = query(
            LoginAuthData {
                username: "legacy-user".into(),
                password: "migrate-me".into(),
                timestamp: 0,
            },
            web::Data::new(pool.clone()),
        )
        .unwrap();
        assert_eq!(user.password_algo, "argon2id");
        assert!(user.password.starts_with("$argon2id$"));
        // Second login still works with the upgraded hash.
        assert!(query(
            LoginAuthData {
                username: "legacy-user".into(),
                password: "migrate-me".into(),
                timestamp: 0,
            },
            web::Data::new(pool),
        )
        .is_ok());
    }
}
