/*
 * @Descripttion: Docker management API handlers
 * @version:
 * @Author: Wynters
 * @Date: 2024-05-25 16:23:02
 * @LastEditTime: 2025-12-15 13:17:54
 * @FilePath: \RustPanel\src\api\v1\docker.rs
 */

use actix_web::{web, HttpResponse};
use serde::Deserialize;

use crate::api::auth::AuthUser;
use crate::api::{ResponseStructure, ResponseStructureError};
use crate::models::docker::{container, network};

// ── Container query parameters ──

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

fn default_tail() -> usize {
    100
}

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

// ── Container handlers ──

pub async fn list_containers(_: AuthUser, query: web::Query<ListContainersQuery>) -> HttpResponse {
    let all = query.all.unwrap_or(false);
    match container::list(all).await {
        Ok(containers) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some(containers),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: err.to_string(),
        }),
    }
}

pub async fn create_container(_: AuthUser, body: web::Json<CreateContainerBody>) -> HttpResponse {
    match container::create(&body.name, &body.image, body.cmd.clone()).await {
        Ok(id) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some(serde_json::json!({"container_id": id})),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: err.to_string(),
        }),
    }
}

pub async fn start_container(_: AuthUser, path: web::Path<ContainerIdPath>) -> HttpResponse {
    match container::start(&path.id).await {
        Ok(_) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some("ok"),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: err.to_string(),
        }),
    }
}

pub async fn stop_container(_: AuthUser, path: web::Path<ContainerIdPath>) -> HttpResponse {
    match container::stop(&path.id).await {
        Ok(_) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some("ok"),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: err.to_string(),
        }),
    }
}

pub async fn restart_container(_: AuthUser, path: web::Path<ContainerIdPath>) -> HttpResponse {
    match container::restart(&path.id).await {
        Ok(_) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some("ok"),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: err.to_string(),
        }),
    }
}

pub async fn remove_container(_: AuthUser, path: web::Path<ContainerIdPath>) -> HttpResponse {
    match container::remove(&path.id).await {
        Ok(_) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some("ok"),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: err.to_string(),
        }),
    }
}

pub async fn pause_container(_: AuthUser, path: web::Path<ContainerIdPath>) -> HttpResponse {
    match container::pause(&path.id).await {
        Ok(_) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some("ok"),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: err.to_string(),
        }),
    }
}

pub async fn unpause_container(_: AuthUser, path: web::Path<ContainerIdPath>) -> HttpResponse {
    match container::unpause(&path.id).await {
        Ok(_) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some("ok"),
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
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some(serde_json::json!({"logs": logs})),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: err.to_string(),
        }),
    }
}

// ── Network handlers ──

pub async fn list_networks(_: AuthUser) -> HttpResponse {
    match network::list().await {
        Ok(networks) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some(networks),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: err.to_string(),
        }),
    }
}

pub async fn create_network(_: AuthUser, body: web::Json<CreateNetworkBody>) -> HttpResponse {
    let driver = body
        .driver
        .clone()
        .unwrap_or_else(|| String::from("bridge"));
    match network::create(
        &body.name,
        &driver,
        body.subnet.as_deref(),
        body.gateway.as_deref(),
    )
    .await
    {
        Ok(id) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some(serde_json::json!({"network_id": id})),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: err.to_string(),
        }),
    }
}

pub async fn remove_network(_: AuthUser, path: web::Path<NetworkIdPath>) -> HttpResponse {
    match network::remove(&path.id).await {
        Ok(_) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some("ok"),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: err.to_string(),
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
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some("ok"),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: err.to_string(),
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
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some("ok"),
        }),
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: err.to_string(),
        }),
    }
}
