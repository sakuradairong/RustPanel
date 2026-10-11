/*
 * @Descripttion:
 * @version:
 * @Author: Wynters
 * @Date: 2024-05-10 15:45:52
 * @LastEditTime: 2024-05-20 19:10:22
 * @FilePath: \RustPanel\src\api\auth.rs
 */
use crate::{
    errors::common::CommonError,
    models::{structure::rp_users::SlimUser, user::find_user_by_id},
    service::{
        db::DBPool,
        global::{CONF, USER_PASSWORD_KEY},
        password::{self, PasswordError},
        sessions,
    },
};
use actix_web::{dev::Payload, web, Error, FromRequest, HttpRequest};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::{future::Future, pin::Pin};

/// Authenticated identity resolved from a revocable session JWT.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: i32,
    pub username: String,
    pub authority: String,
    pub created_at: chrono::NaiveDateTime,
    pub jti: String,
    pub must_change_password: bool,
    pub token_version: i32,
    pub expires_at_unix: i64,
}

impl From<(SlimUser, String, bool, i32, i64)> for AuthUser {
    fn from((user, jti, must_change_password, token_version, expires_at_unix): (SlimUser, String, bool, i32, i64)) -> Self {
        Self {
            id: user.id,
            username: user.username,
            authority: user.authority,
            created_at: user.created_at,
            jti,
            must_change_password,
            token_version,
            expires_at_unix,
        }
    }
}

/// Access-token claims: immutable user id only — no password material.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Claims {
    /// Opaque user id string.
    pub(crate) sub: String,
    pub(crate) jti: String,
    pub(crate) iat: usize,
    pub(crate) exp: usize,
    /// Snapshot of `rp_users.token_version` at issue time.
    pub(crate) ver: i32,
}

/// Resolve identity against session table + current database user state.
pub(crate) fn authenticate_token(token: &str, pool: &DBPool) -> Result<AuthUser, CommonError> {
    let claims = decode_jwt(token)
        .map_err(|_| CommonError::Unauthorized("Invalid credentials".into()))?;
    let user_id: i32 = claims
        .sub
        .parse()
        .map_err(|_| CommonError::Unauthorized("Invalid credentials".into()))?;

    {
        // Release the pool connection before the next checkout in find_user_by_id.
        let mut conn = pool.get().map_err(|_| {
            CommonError::ServiceUnavailable("Authentication database unavailable".into())
        })?;
        sessions::validate_session(&mut conn, &claims.jti, user_id)?;
    }

    let user = find_user_by_id(user_id, pool)
        .map_err(|_| CommonError::ServiceUnavailable("Authentication database unavailable".into()))?
        .ok_or_else(|| CommonError::Unauthorized("Invalid credentials".into()))?;
    if user.status != 0 || user.token_version != claims.ver {
        return Err(CommonError::Unauthorized("Invalid credentials".into()));
    }
    let must_change = user.must_change_password != 0;
    Ok(AuthUser::from((user.into(), claims.jti, must_change, claims.ver, claims.exp as i64)))
}

fn path_allows_must_change(path: &str) -> bool {
    matches!(
        path,
        "/api/v1/user/me"
            | "/api/v1/user/change-password"
            | "/api/v1/login/logout"
            | "/api/v1/login/security_dir"
    )
}

impl FromRequest for AuthUser {
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<AuthUser, Error>>>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        let token = req
            .headers()
            .get("Authorization")
            .and_then(|value| value.to_str().ok())
            .map(|value| value.strip_prefix("Bearer ").unwrap_or(value).to_owned());
        let pool = req.app_data::<web::Data<DBPool>>().cloned();
        let path = req.path().to_string();
        Box::pin(async move {
            let token =
                token.ok_or_else(|| CommonError::Unauthorized("Invalid credentials".into()))?;
            let pool = pool.ok_or_else(|| {
                CommonError::ServiceUnavailable("Authentication database unavailable".into())
            })?;
            let user = web::block(move || authenticate_token(&token, pool.get_ref()))
                .await
                .map_err(|_| {
                    CommonError::ServiceUnavailable("Authentication service unavailable".into())
                })?
                .map_err(Error::from)?;
            if user.must_change_password && !path_allows_must_change(&path) {
                return Err(CommonError::Forbidden(
                    "Password change required before continuing".into(),
                )
                .into());
            }
            Ok(user)
        })
    }
}

/// Requires the `user:admin` permission (resolved from DB roles on each request).
pub struct AdminUser(pub AuthUser);

impl std::ops::Deref for AdminUser {
    type Target = AuthUser;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromRequest for AdminUser {
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Error>>>>;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let user_fut = AuthUser::from_request(req, payload);
        let pool = req.app_data::<web::Data<DBPool>>().cloned();
        Box::pin(async move {
            let user = user_fut.await?;
            let pool = pool.ok_or_else(|| {
                CommonError::ServiceUnavailable("Authorization database unavailable".into())
            })?;
            crate::service::rbac::require_perm(pool.get_ref(), &user, crate::service::rbac::PERM_USER_ADMIN)?;
            Ok(Self(user))
        })
    }
}

#[derive(Debug)]
pub enum AuthError {
    HashError,
    VerificationError,
}

impl From<PasswordError> for AuthError {
    fn from(_: PasswordError) -> Self {
        AuthError::VerificationError
    }
}

pub fn verify_password(
    hash: &str,
    salt: &str,
    algo: &str,
    password: &str,
) -> Result<bool, AuthError> {
    Ok(password::verify_stored(hash, salt, algo, password)?.0)
}

pub fn generate_jwt(
    user_id: i32,
    jti: &str,
    token_version: i32,
    exp_unix: i64,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as usize)
        .unwrap_or(0);
    encode(
        &Header::new(Algorithm::HS256),
        &Claims {
            sub: user_id.to_string(),
            jti: jti.to_owned(),
            iat: now,
            exp: exp_unix as usize,
            ver: token_version,
        },
        &EncodingKey::from_secret(USER_PASSWORD_KEY.as_bytes()),
    )
}

pub(crate) fn decode_jwt(token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let key = DecodingKey::from_secret(USER_PASSWORD_KEY.as_bytes());
    let mut validation = Validation::default();
    validation.validate_exp = true;
    validation.algorithms = vec![Algorithm::HS256];
    let token_data = decode::<Claims>(token, &key, &validation)?;
    Ok(token_data.claims)
}

/// Session TTL used when issuing tokens (seconds).
pub fn session_ttl_seconds() -> i64 {
    i64::from(CONF.app.session_ttl) * 60 * 60
}
