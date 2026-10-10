//! Origin/Referer CSRF checks for cookie-authenticated mutating requests (RP-10).
use std::{
    future::{ready, Ready},
    rc::Rc,
};

use actix_web::{
    body::EitherBody,
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    http::{header, Method},
    Error, HttpResponse,
};
use futures_util::future::LocalBoxFuture;

use crate::api::ResponseStructureError;

pub struct CsrfProtect;

impl<S, B> Transform<S, ServiceRequest> for CsrfProtect
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type InitError = ();
    type Transform = CsrfProtectMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(CsrfProtectMiddleware {
            service: Rc::new(service),
        }))
    }
}

pub struct CsrfProtectMiddleware<S> {
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for CsrfProtectMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let service = Rc::clone(&self.service);
        Box::pin(async move {
            if let Err(resp) = check_csrf(&req) {
                return Ok(req.into_response(resp).map_into_right_body());
            }
            let res = service.call(req).await?;
            Ok(res.map_into_left_body())
        })
    }
}

fn is_mutating(method: &Method) -> bool {
    matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

fn has_session_cookie(req: &ServiceRequest) -> bool {
    req.headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(|c| c.split(';').any(|part| part.trim().starts_with("token=")))
        .unwrap_or(false)
}

fn origin_host(value: &str) -> Option<String> {
    // Accept full Origin URL or Referer URL; compare host:port when present.
    let value = value.trim();
    if value == "null" {
        return None;
    }
    let url = value.strip_prefix("https://").or_else(|| value.strip_prefix("http://"))?;
    let host = url.split('/').next()?.to_ascii_lowercase();
    Some(host)
}

fn request_host(req: &ServiceRequest) -> Option<String> {
    req.headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(|h| h.to_ascii_lowercase())
}

fn check_csrf(req: &ServiceRequest) -> Result<(), HttpResponse> {
    if !is_mutating(req.method()) || !has_session_cookie(req) {
        return Ok(());
    }
    // Bearer-only API clients without the session cookie are not subject to cookie CSRF.
    let Some(host) = request_host(req) else {
        return Err(forbidden("Missing Host header for CSRF check"));
    };
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .and_then(origin_host);
    let referer = req
        .headers()
        .get(header::REFERER)
        .and_then(|v| v.to_str().ok())
        .and_then(origin_host);
    let candidate = origin.or(referer);
    match candidate {
        Some(ref h) if hosts_match(h, &host) => Ok(()),
        Some(_) => Err(forbidden("CSRF origin/referer mismatch")),
        None => Err(forbidden(
            "CSRF check failed: Origin or Referer required for cookie-authenticated writes",
        )),
    }
}

fn hosts_match(a: &str, b: &str) -> bool {
    a == b
}

fn forbidden(msg: &str) -> HttpResponse {
    ResponseStructureError::forbidden(msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_origin_host() {
        assert_eq!(
            origin_host("https://panel.example:8080/path"),
            Some("panel.example:8080".into())
        );
        assert_eq!(origin_host("http://127.0.0.1/"), Some("127.0.0.1".into()));
        assert!(origin_host("null").is_none());
    }
}
