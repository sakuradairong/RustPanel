use actix_web::{web, HttpResponse};
use serde::Deserialize;

use crate::api::auth::AuthUser;
use crate::api::{ResponseStructure, ResponseStructureError};
use crate::models::webserver::apache::{self, ApacheLayout};
use crate::models::webserver::caddy::{self, CaddyLayout};
use crate::models::webserver::config::{self, SiteSpec, SslMode};
use crate::models::webserver::SiteError;

#[derive(Deserialize)]
pub struct SitePath {
    pub name: String,
}

#[derive(Deserialize)]
pub struct CreateSiteBody {
    pub server_name: String,
    pub root: Option<String>,
    pub listen: Option<u16>,
    pub proxy_pass: Option<String>,
    pub ssl: Option<String>,
    pub index: Option<String>,
    pub extra_config: Option<String>,
}

pub async fn apache_status(_: AuthUser) -> HttpResponse {
    server_status("apache", apache::detect().await, apache_status_of).await
}

pub async fn caddy_status(_: AuthUser) -> HttpResponse {
    server_status("caddy", caddy::detect().await, caddy_status_of).await
}

pub async fn list_apache_sites(_: AuthUser) -> HttpResponse {
    if let Some(response) = reject_non_linux() {
        return response;
    }
    let Some(layout) = apache::detect().await else {
        return ok(Vec::<crate::models::webserver::apache::ManagedSite>::new());
    };
    match apache::list_sites(&layout).await {
        Ok(sites) => ok(sites),
        Err(err) => site_error(err),
    }
}

pub async fn list_caddy_sites(_: AuthUser) -> HttpResponse {
    if let Some(response) = reject_non_linux() {
        return response;
    }
    let Some(layout) = caddy::detect().await else {
        return ok(Vec::<crate::models::webserver::apache::ManagedSite>::new());
    };
    match caddy::list_sites(&layout).await {
        Ok(sites) => ok(sites),
        Err(err) => site_error(err),
    }
}

pub async fn create_apache_site(_: AuthUser, body: web::Json<CreateSiteBody>) -> HttpResponse {
    let layout = match require_apache().await {
        Ok(layout) => layout,
        Err(response) => return response,
    };
    let spec = match spec_from_body(&body) {
        Ok(spec) => spec,
        Err(err) => return site_error(err),
    };
    let written = match apache::create_site(&layout, &spec).await {
        Ok(written) => written,
        Err(err) => return site_error(err),
    };
    if let Err(err) = apache::test_config(&layout).await {
        let _ = apache::delete_site(&layout, &written.name).await;
        return bad(&format!("apache configuration test failed: {err}"));
    }
    finish_create(&written, apache::reload(&layout).await, "apache")
}

pub async fn create_caddy_site(_: AuthUser, body: web::Json<CreateSiteBody>) -> HttpResponse {
    let layout = match require_caddy().await {
        Ok(layout) => layout,
        Err(response) => return response,
    };
    let spec = match spec_from_body(&body) {
        Ok(spec) => spec,
        Err(err) => return site_error(err),
    };
    let written = match caddy::create_site(&layout, &spec).await {
        Ok(written) => written,
        Err(err) => return site_error(err),
    };
    if let Err(err) = caddy::test_config(&layout).await {
        let _ = caddy::delete_site(&layout, &written.name).await;
        return bad(&format!("caddy configuration test failed: {err}"));
    }
    finish_create(&written, caddy::reload(&layout).await, "caddy")
}

pub async fn delete_apache_site(_: AuthUser, path: web::Path<SitePath>) -> HttpResponse {
    let layout = match require_apache().await {
        Ok(layout) => layout,
        Err(response) => return response,
    };
    if let Err(err) = apache::delete_site(&layout, &path.name).await {
        return site_error(err);
    }
    finish_reload(
        apache::test_config(&layout).await,
        apache::reload(&layout).await,
        "apache",
        "Site deleted",
    )
}

pub async fn delete_caddy_site(_: AuthUser, path: web::Path<SitePath>) -> HttpResponse {
    let layout = match require_caddy().await {
        Ok(layout) => layout,
        Err(response) => return response,
    };
    if let Err(err) = caddy::delete_site(&layout, &path.name).await {
        return site_error(err);
    }
    finish_reload(
        caddy::test_config(&layout).await,
        caddy::reload(&layout).await,
        "caddy",
        "Site deleted",
    )
}

pub async fn enable_apache_site(_: AuthUser, path: web::Path<SitePath>) -> HttpResponse {
    change_apache(&path.name, true).await
}

pub async fn disable_apache_site(_: AuthUser, path: web::Path<SitePath>) -> HttpResponse {
    change_apache(&path.name, false).await
}

pub async fn enable_caddy_site(_: AuthUser, path: web::Path<SitePath>) -> HttpResponse {
    change_caddy(&path.name, true).await
}

pub async fn disable_caddy_site(_: AuthUser, path: web::Path<SitePath>) -> HttpResponse {
    change_caddy(&path.name, false).await
}

pub async fn reload_apache(_: AuthUser) -> HttpResponse {
    let layout = match require_apache().await {
        Ok(layout) => layout,
        Err(response) => return response,
    };
    match apache::reload(&layout).await {
        Ok(message) => ok(serde_json::json!({ "reload": message })),
        Err(err) => internal(&err),
    }
}

pub async fn reload_caddy(_: AuthUser) -> HttpResponse {
    let layout = match require_caddy().await {
        Ok(layout) => layout,
        Err(response) => return response,
    };
    match caddy::reload(&layout).await {
        Ok(message) => ok(serde_json::json!({ "reload": message })),
        Err(err) => internal(&err),
    }
}

async fn change_apache(name: &str, enabled: bool) -> HttpResponse {
    let layout = match require_apache().await {
        Ok(layout) => layout,
        Err(response) => return response,
    };
    if let Err(err) = apache::set_enabled(&layout, name, enabled).await {
        return site_error(err);
    }
    let _ = apache::reload(&layout).await;
    ok_empty()
}

async fn change_caddy(name: &str, enabled: bool) -> HttpResponse {
    let layout = match require_caddy().await {
        Ok(layout) => layout,
        Err(response) => return response,
    };
    if let Err(err) = caddy::set_enabled(&layout, name, enabled).await {
        return site_error(err);
    }
    let _ = caddy::reload(&layout).await;
    ok_empty()
}

async fn server_status<T, F, Fut>(name: &str, detected: Option<T>, read: F) -> HttpResponse
where
    F: FnOnce(T) -> Fut,
    Fut: std::future::Future<Output = HttpResponse>,
{
    if let Some(response) = reject_non_linux() {
        return response;
    }
    match detected {
        Some(layout) => read(layout).await,
        None => ok(serde_json::json!({
            "installed": false,
            "running": false,
            "version": null,
            "config_test": format!("{name} not found"),
        })),
    }
}

async fn apache_status_of(layout: ApacheLayout) -> HttpResponse {
    let version = apache::version(&layout).await;
    let running = apache::is_running(&layout).await;
    let config_test = apache::test_config(&layout).await.unwrap_or_else(|err| err);
    ok(serde_json::json!({
        "installed": true,
        "running": running,
        "version": version,
        "config_test": config_test,
    }))
}

async fn caddy_status_of(layout: CaddyLayout) -> HttpResponse {
    let version = caddy::version(&layout).await;
    let running = caddy::is_running(&layout).await;
    let config_test = caddy::test_config(&layout).await.unwrap_or_else(|err| err);
    ok(serde_json::json!({
        "installed": true,
        "running": running,
        "version": version,
        "config_test": config_test,
    }))
}

async fn require_apache() -> Result<ApacheLayout, HttpResponse> {
    if let Some(response) = reject_non_linux() {
        return Err(response);
    }
    apache::detect()
        .await
        .ok_or_else(|| bad("apache is not installed"))
}

async fn require_caddy() -> Result<CaddyLayout, HttpResponse> {
    if let Some(response) = reject_non_linux() {
        return Err(response);
    }
    caddy::detect()
        .await
        .ok_or_else(|| bad("caddy is not installed"))
}

fn spec_from_body(body: &CreateSiteBody) -> Result<SiteSpec, SiteError> {
    let ssl = SslMode::parse(body.ssl.as_deref().unwrap_or(""));
    let listen = body.listen.unwrap_or(if ssl.is_tls() { 443 } else { 80 });
    let root = blank_to_none(body.root.as_deref());
    let proxy_pass = blank_to_none(body.proxy_pass.as_deref());
    let extra_config = blank_to_none(body.extra_config.as_deref());
    let spec = SiteSpec {
        server_name: body.server_name.trim().to_string(),
        root,
        listen,
        proxy_pass,
        ssl,
        index: blank_to_none(body.index.as_deref())
            .unwrap_or_else(|| "index.html index.htm".into()),
        extra_config,
    };
    config::prepare_spec(&spec)?;
    Ok(spec)
}

fn blank_to_none(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn finish_create(
    written: &apache::WrittenSite,
    reload: Result<String, String>,
    server: &str,
) -> HttpResponse {
    match reload {
        Ok(message) => ok(serde_json::json!({
            "config": written.config,
            "path": written.path,
            "reload": message,
        })),
        Err(err) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: format!("Config created but {server} reload failed"),
            data: Some(serde_json::json!({
                "config": written.config,
                "path": written.path,
                "reload_error": err,
            })),
        }),
    }
}

fn finish_reload(
    test: Result<String, String>,
    reload: Result<String, String>,
    server: &str,
    action: &str,
) -> HttpResponse {
    if let Err(err) = test {
        return ok(
            serde_json::json!({ "config_test": err, "message": format!("{action} but {server} config test failed") }),
        );
    }
    match reload {
        Ok(message) => ok(serde_json::json!({ "reload": message })),
        Err(err) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: format!("{action} but {server} reload failed"),
            data: Some(serde_json::json!({ "reload_error": err })),
        }),
    }
}

fn reject_non_linux() -> Option<HttpResponse> {
    if cfg!(target_os = "linux") {
        None
    } else {
        Some(bad("Web server management is only supported on Linux"))
    }
}

fn ok<T: serde::Serialize>(data: T) -> HttpResponse {
    HttpResponse::Ok().json(ResponseStructure {
        success: true,
        code: 200,
        message: String::from("success"),
        data: Some(data),
    })
}

fn ok_empty() -> HttpResponse {
    HttpResponse::Ok().json(ResponseStructure::<()> {
        success: true,
        code: 200,
        message: String::from("success"),
        data: None,
    })
}

fn bad(message: &str) -> HttpResponse {
    HttpResponse::BadRequest().json(ResponseStructureError {
        success: false,
        code: 400,
        message: message.to_string(),
    })
}

fn internal(message: &str) -> HttpResponse {
    HttpResponse::InternalServerError().json(ResponseStructureError {
        success: false,
        code: 500,
        message: message.to_string(),
    })
}

fn site_error(err: SiteError) -> HttpResponse {
    match err {
        SiteError::Invalid(message) => bad(&message),
        SiteError::NotFound(message) => HttpResponse::NotFound().json(ResponseStructureError {
            success: false,
            code: 404,
            message,
        }),
        SiteError::Io(message) => internal(&message),
    }
}
