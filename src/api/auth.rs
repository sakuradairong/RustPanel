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
    },
};
use actix_web::{dev::Payload, web, Error, FromRequest, HttpRequest};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::{future::Future, pin::Pin};

// we need the same data
// simple aliasing makes the intentions clear and its more readable
pub type AuthUser = SlimUser;

/// Resolve identity against current database state, including role and disabled status.
/// Legacy JWT payloads remain accepted during the upcoming session/schema migration.
pub(crate) fn authenticate_token(token: &str, pool: &DBPool) -> Result<AuthUser, CommonError> {
    let claims = decode_jwt(token)
        .map_err(|_| CommonError::Unauthorized("Invalid credentials".into()))?;
    let subject: SlimUser = serde_json::from_str(&claims.sub)
        .map_err(|_| CommonError::Unauthorized("Invalid credentials".into()))?;
    let user = find_user_by_id(subject.id, pool)
        .map_err(|_| CommonError::ServiceUnavailable("Authentication database unavailable".into()))?
        .ok_or_else(|| CommonError::Unauthorized("Invalid credentials".into()))?;
    if user.status != 0 || user.password != subject.password {
        return Err(CommonError::Unauthorized("Invalid credentials".into()));
    }
    // Never authorize using a stale role from the JWT.
    Ok(user.into())
}

impl FromRequest for AuthUser {
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<AuthUser, Error>>>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        let token = req.headers().get("Authorization")
            .and_then(|value| value.to_str().ok())
            .map(|value| value.strip_prefix("Bearer ").unwrap_or(value).to_owned());
        let pool = req.app_data::<web::Data<DBPool>>().cloned();
        Box::pin(async move {
            let token = token.ok_or_else(|| CommonError::Unauthorized("Invalid credentials".into()))?;
            let pool = pool.ok_or_else(|| CommonError::ServiceUnavailable("Authentication database unavailable".into()))?;
            web::block(move || authenticate_token(&token, pool.get_ref()))
                .await
                .map_err(|_| CommonError::ServiceUnavailable("Authentication service unavailable".into()))?
                .map_err(Error::from)
        })
    }
}

/// Temporary coarse authorization until the resource-scoped RBAC migration lands.
pub struct AdminUser;

impl FromRequest for AdminUser {
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Error>>>>;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let user = AuthUser::from_request(req, payload);
        Box::pin(async move {
            if user.await?.authority != "admin" {
                return Err(CommonError::Forbidden("Administrator access required".into()).into());
            }
            Ok(Self)
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

/// Verify a stored password. Prefer [`password::verify_stored`] for rehash decisions.
pub fn verify_password(
    hash: &str,
    salt: &str,
    algo: &str,
    password: &str,
) -> Result<bool, AuthError> {
    Ok(password::verify_stored(hash, salt, algo, password)?.0)
}

#[deprecated(note = "use verify_password with password_algo")]
pub fn verify_password_sha1(hash: &str, salt: &str, password: &str) -> Result<bool, AuthError> {
    Ok(password::verify_sha1(hash, salt, password))
}


#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Claims {
    pub(crate) sub: String,
    pub(crate) exp: usize,
}

pub fn generate_jwt(subject: &str) -> Result<String, jsonwebtoken::errors::Error> {
    // 生成 JWT
    Ok(encode(
        &Header::new(Algorithm::HS256),
        &Claims {
            sub: String::from(subject),
            exp: (match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
                Ok(d) => d.as_secs(),
                Err(_) => 0,
            } + (CONF.app.session_ttl * 60 * 60) as u64) as usize,
        },
        &EncodingKey::from_secret(USER_PASSWORD_KEY.as_bytes()),
    )?)
}

pub(crate) fn decode_jwt(token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let key = DecodingKey::from_secret(USER_PASSWORD_KEY.as_bytes());

    // 手动创建 Validation 实例，并设置所需的字段
    let mut validation = Validation::default();
    validation.validate_exp = true;
    validation.algorithms = vec![Algorithm::HS256];

    let token_data = decode::<Claims>(token, &key, &validation)?;


    Ok(token_data.claims)
}
