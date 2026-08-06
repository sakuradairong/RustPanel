use actix_web::{web, HttpResponse};
use serde::Deserialize;

use crate::api::auth::AuthUser;
use crate::api::{ResponseStructure, ResponseStructureError};
use crate::models::docker::{container, image, network, volume};

// ── Common path structs ──

#[derive(Deserialize)]
pub struct ListContainersQuery {
    pub all: Option<bool>,
}

#[derive(Deserialize)]
pub struct ContainerIdPath {
    pub id: String,
}

#[derive(Deserialize)]
pub struct CreateContainerBody {
    pub name: String,
    pub image: String,
    pub cmd: Option<Vec<String>>,
    pub ports: Option<Vec<String>>,
    pub env: Option<Vec<String>>,
    pub restart_policy: Option<String>,
    pub network_mode: Option<String>,
}

#[derive(Deserialize)]
pub struct ContainerLogsQuery {
    #[serde(default = "default_tail")]
    pub tail: usize,
}

fn default_tail() -> usize { 100 }

// ── Network query parameters ──

#[derive(Deserialize)]
pub struct NetworkIdPath {
    pub id: String,
}

#[derive(Deserialize)]
pub struct CreateNetworkBody {
    pub name: String,
    pub driver: Option<String>,
    pub subnet: Option<String>,
    pub gateway: Option<String>,
}

#[derive(Deserialize)]
pub struct NetworkConnectBody {
    pub container_id: String,
}

// ── Image handlers ──

#[derive(Deserialize)]
pub struct PullImageBody {
    pub image: String,
}

#[derive(Deserialize)]
pub struct PruneImagesBody {
    /// When true, only prune dangling images (default true).
    pub dangling_only: Option<bool>,
}

#[derive(Deserialize)]
pub struct ExecContainerBody {
    /// Shell command string, e.g. `ls -la /`. Runs via `/bin/sh -c`.
    pub cmd: String,
}

pub async fn list_images(_: AuthUser) -> HttpResponse {
    match image::list().await {
        Ok(images) => HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200, message: String::from("success"),
            data: Some(images),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500, message: err.to_string(),
        }),
    }
}

pub async fn pull_image(_: AuthUser, body: web::Json<PullImageBody>) -> HttpResponse {
    match image::pull(&body.image).await {
        Ok(output) => HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200, message: String::from("success"),
            data: Some(serde_json::json!({"output": output})),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500, message: err.to_string(),
        }),
    }
}

pub async fn remove_image(_: AuthUser, path: web::Path<ContainerIdPath>) -> HttpResponse {
    match image::remove(&path.id).await {
        Ok(_) => HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200, message: String::from("success"),
            data: None::<()>,
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500, message: err.to_string(),
        }),
    }
}

pub async fn prune_images(_: AuthUser, body: web::Json<PruneImagesBody>) -> HttpResponse {
    let dangling_only = body.dangling_only.unwrap_or(true);
    match image::prune(dangling_only).await {
        Ok(result) => HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200, message: String::from("success"),
            data: Some(result),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500, message: err.to_string(),
        }),
    }
}

// ── Container inspect/stats ──

pub async fn inspect_container(_: AuthUser, path: web::Path<ContainerIdPath>) -> HttpResponse {
    match container::inspect(&path.id).await {
        Ok(info) => HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200, message: String::from("success"),
            data: Some(info),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500, message: err.to_string(),
        }),
    }
}

pub async fn container_stats(_: AuthUser, path: web::Path<ContainerIdPath>) -> HttpResponse {
    match container::stats(&path.id).await {
        Ok(info) => HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200, message: String::from("success"),
            data: Some(info),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500, message: err.to_string(),
        }),
    }
}

// ── Container handlers ──

pub async fn list_containers(_: AuthUser, query: web::Query<ListContainersQuery>) -> HttpResponse {
    let all = query.all.unwrap_or(false);
    match container::list(all).await {
        Ok(containers) => HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200, message: String::from("success"),
            data: Some(containers),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500, message: err.to_string(),
        }),
    }
}

pub async fn create_container(_: AuthUser, body: web::Json<CreateContainerBody>) -> HttpResponse {
    match container::create(
        &body.name,
        &body.image,
        body.cmd.clone(),
        body.ports.clone().unwrap_or_default(),
        body.env.clone().unwrap_or_default(),
        body.restart_policy.clone(),
        body.network_mode.clone(),
    )
    .await
    {
        Ok(id) => HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200, message: String::from("success"),
            data: Some(serde_json::json!({"container_id": id})),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500, message: err.to_string(),
        }),
    }
}

pub async fn start_container(_: AuthUser, path: web::Path<ContainerIdPath>) -> HttpResponse {
    match container::start(&path.id).await {
        Ok(_) => HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200, message: String::from("success"),
            data: Some("ok"),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500, message: err.to_string(),
        }),
    }
}

pub async fn stop_container(_: AuthUser, path: web::Path<ContainerIdPath>) -> HttpResponse {
    match container::stop(&path.id).await {
        Ok(_) => HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200, message: String::from("success"),
            data: Some("ok"),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500, message: err.to_string(),
        }),
    }
}

pub async fn restart_container(_: AuthUser, path: web::Path<ContainerIdPath>) -> HttpResponse {
    match container::restart(&path.id).await {
        Ok(_) => HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200, message: String::from("success"),
            data: Some("ok"),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500, message: err.to_string(),
        }),
    }
}

pub async fn remove_container(_: AuthUser, path: web::Path<ContainerIdPath>) -> HttpResponse {
    match container::remove(&path.id).await {
        Ok(_) => HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200, message: String::from("success"),
            data: Some("ok"),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500, message: err.to_string(),
        }),
    }
}

pub async fn pause_container(_: AuthUser, path: web::Path<ContainerIdPath>) -> HttpResponse {
    match container::pause(&path.id).await {
        Ok(_) => HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200, message: String::from("success"),
            data: Some("ok"),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500, message: err.to_string(),
        }),
    }
}

pub async fn unpause_container(_: AuthUser, path: web::Path<ContainerIdPath>) -> HttpResponse {
    match container::unpause(&path.id).await {
        Ok(_) => HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200, message: String::from("success"),
            data: Some("ok"),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500, message: err.to_string(),
        }),
    }
}

pub async fn exec_container(
    _: AuthUser,
    path: web::Path<ContainerIdPath>,
    body: web::Json<ExecContainerBody>,
) -> HttpResponse {
    let cmd = body.cmd.trim();
    if cmd.is_empty() {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false,
            code: 400,
            message: String::from("cmd is required"),
        });
    }
    let argv = vec![
        String::from("/bin/sh"),
        String::from("-c"),
        cmd.to_string(),
    ];
    match container::exec(&path.id, argv).await {
        Ok(output) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some(serde_json::json!({"output": output})),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: err.to_string(),
        }),
    }
}

pub async fn get_container_logs(
    _: AuthUser,
    path: web::Path<ContainerIdPath>,
    query: web::Query<ContainerLogsQuery>,
) -> HttpResponse {
    match container::logs(&path.id, query.tail).await {
        Ok(logs) => HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200, message: String::from("success"),
            data: Some(serde_json::json!({"logs": logs})),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500, message: err.to_string(),
        }),
    }
}

// ── Network handlers ──

pub async fn list_networks(_: AuthUser) -> HttpResponse {
    match network::list().await {
        Ok(networks) => HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200, message: String::from("success"),
            data: Some(networks),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500, message: err.to_string(),
        }),
    }
}

pub async fn create_network(_: AuthUser, body: web::Json<CreateNetworkBody>) -> HttpResponse {
    let driver = body.driver.clone().unwrap_or_else(|| String::from("bridge"));
    match network::create(&body.name, &driver, body.subnet.as_deref(), body.gateway.as_deref()).await {
        Ok(id) => HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200, message: String::from("success"),
            data: Some(serde_json::json!({"network_id": id})),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500, message: err.to_string(),
        }),
    }
}

pub async fn remove_network(_: AuthUser, path: web::Path<NetworkIdPath>) -> HttpResponse {
    match network::remove(&path.id).await {
        Ok(_) => HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200, message: String::from("success"),
            data: Some("ok"),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500, message: err.to_string(),
        }),
    }
}

pub async fn connect_network(
    _: AuthUser,
    path: web::Path<NetworkIdPath>,
    body: web::Json<NetworkConnectBody>,
) -> HttpResponse {
    match network::connect(&path.id, &body.container_id).await {
        Ok(_) => HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200, message: String::from("success"),
            data: Some("ok"),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500, message: err.to_string(),
        }),
    }
}

pub async fn disconnect_network(
    _: AuthUser,
    path: web::Path<NetworkIdPath>,
    body: web::Json<NetworkConnectBody>,
) -> HttpResponse {
    match network::disconnect(&path.id, &body.container_id).await {
        Ok(_) => HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200, message: String::from("success"),
            data: Some("ok"),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500, message: err.to_string(),
        }),
    }
}

// ── Volume handlers ──


#[derive(Deserialize)]
pub struct CreateVolumeBody {
    pub name: String,
    #[serde(default)]
    pub driver: Option<String>,
}

pub async fn list_volumes(_: AuthUser) -> HttpResponse {
    match volume::list().await {
        Ok(volumes) => HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200, message: String::from("success"),
            data: Some(volumes),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500, message: err.to_string(),
        }),
    }
}

pub async fn create_volume(_: AuthUser, body: web::Json<CreateVolumeBody>) -> HttpResponse {
    let driver = body.driver.as_deref().unwrap_or("local");
    match volume::create(&body.name, driver).await {
        Ok(name) => HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200, message: String::from("success"),
            data: Some(serde_json::json!({"name": name})),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500, message: err.to_string(),
        }),
    }
}

pub async fn remove_volume(_: AuthUser, path: web::Path<String>) -> HttpResponse {
    let name = path.into_inner();
    match volume::remove(&name).await {
        Ok(_) => HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200, message: String::from("success"),
            data: Some("ok"),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500, message: err.to_string(),
        }),
    }
}

pub async fn inspect_volume(_: AuthUser, path: web::Path<String>) -> HttpResponse {
    let name = path.into_inner();
    match volume::inspect(&name).await {
        Ok(info) => HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200, message: String::from("success"),
            data: Some(info),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500, message: err.to_string(),
        }),
    }
}
