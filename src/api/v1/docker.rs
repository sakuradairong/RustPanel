use actix_web::{web, HttpResponse, ResponseError};
use serde::Deserialize;

use crate::api::auth::AuthUser;
use crate::api::{ResponseStructure, ResponseStructureError};
use crate::models::docker::{container, image, network, volume};
use crate::service::db::DBPool;
use crate::service::rbac::{self, PERM_DOCKER_READ, PERM_DOCKER_WRITE};

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

pub async fn list_images(user: AuthUser, pool: web::Data<DBPool>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_DOCKER_READ) {
        return err.error_response();
    }
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

pub async fn pull_image(user: AuthUser, pool: web::Data<DBPool>, body: web::Json<PullImageBody>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_DOCKER_WRITE) {
        return err.error_response();
    }
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

pub async fn remove_image(user: AuthUser, pool: web::Data<DBPool>, path: web::Path<ContainerIdPath>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_DOCKER_WRITE) {
        return err.error_response();
    }
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

// ── Container inspect/stats ──

pub async fn inspect_container(user: AuthUser, pool: web::Data<DBPool>, path: web::Path<ContainerIdPath>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_DOCKER_READ) {
        return err.error_response();
    }
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

pub async fn container_stats(user: AuthUser, pool: web::Data<DBPool>, path: web::Path<ContainerIdPath>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_DOCKER_READ) {
        return err.error_response();
    }
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

pub async fn list_containers(user: AuthUser, pool: web::Data<DBPool>, query: web::Query<ListContainersQuery>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_DOCKER_READ) {
        return err.error_response();
    }
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

pub async fn create_container(user: AuthUser, pool: web::Data<DBPool>, body: web::Json<CreateContainerBody>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_DOCKER_WRITE) {
        return err.error_response();
    }
    match container::create(&body.name, &body.image, body.cmd.clone()).await {
        Ok(id) => HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200, message: String::from("success"),
            data: Some(serde_json::json!({"container_id": id})),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500, message: err.to_string(),
        }),
    }
}

pub async fn start_container(user: AuthUser, pool: web::Data<DBPool>, path: web::Path<ContainerIdPath>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_DOCKER_WRITE) {
        return err.error_response();
    }
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

pub async fn stop_container(user: AuthUser, pool: web::Data<DBPool>, path: web::Path<ContainerIdPath>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_DOCKER_WRITE) {
        return err.error_response();
    }
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

pub async fn restart_container(user: AuthUser, pool: web::Data<DBPool>, path: web::Path<ContainerIdPath>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_DOCKER_WRITE) {
        return err.error_response();
    }
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

pub async fn remove_container(user: AuthUser, pool: web::Data<DBPool>, path: web::Path<ContainerIdPath>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_DOCKER_WRITE) {
        return err.error_response();
    }
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

pub async fn pause_container(user: AuthUser, pool: web::Data<DBPool>, path: web::Path<ContainerIdPath>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_DOCKER_WRITE) {
        return err.error_response();
    }
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

pub async fn unpause_container(user: AuthUser, pool: web::Data<DBPool>, path: web::Path<ContainerIdPath>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_DOCKER_WRITE) {
        return err.error_response();
    }
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

pub async fn get_container_logs(
    user: AuthUser,
    pool: web::Data<DBPool>,
    path: web::Path<ContainerIdPath>,
    query: web::Query<ContainerLogsQuery>,
) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_DOCKER_READ) {
        return err.error_response();
    }
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

pub async fn list_networks(user: AuthUser, pool: web::Data<DBPool>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_DOCKER_READ) {
        return err.error_response();
    }
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

pub async fn create_network(user: AuthUser, pool: web::Data<DBPool>, body: web::Json<CreateNetworkBody>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_DOCKER_WRITE) {
        return err.error_response();
    }
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

pub async fn remove_network(user: AuthUser, pool: web::Data<DBPool>, path: web::Path<NetworkIdPath>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_DOCKER_WRITE) {
        return err.error_response();
    }
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
    user: AuthUser,
    pool: web::Data<DBPool>,
    path: web::Path<NetworkIdPath>,
    body: web::Json<NetworkConnectBody>,
) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_DOCKER_WRITE) {
        return err.error_response();
    }
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
    user: AuthUser,
    pool: web::Data<DBPool>,
    path: web::Path<NetworkIdPath>,
    body: web::Json<NetworkConnectBody>,
) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_DOCKER_WRITE) {
        return err.error_response();
    }
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

pub async fn list_volumes(user: AuthUser, pool: web::Data<DBPool>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_DOCKER_READ) {
        return err.error_response();
    }
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

pub async fn create_volume(user: AuthUser, pool: web::Data<DBPool>, body: web::Json<CreateVolumeBody>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_DOCKER_WRITE) {
        return err.error_response();
    }
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

pub async fn remove_volume(user: AuthUser, pool: web::Data<DBPool>, path: web::Path<String>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_DOCKER_WRITE) {
        return err.error_response();
    }
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

pub async fn inspect_volume(user: AuthUser, pool: web::Data<DBPool>, path: web::Path<String>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_DOCKER_READ) {
        return err.error_response();
    }
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
