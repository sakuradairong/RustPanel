use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};

use crate::api::auth::AuthUser;
use crate::api::{ResponseStructure, ResponseStructureError};
use crate::models::system::installer::{self, SoftwareType};

#[derive(Serialize)]
struct SoftwareInfo {
    name: String,
    description: String,
    versions_available: bool,
}

pub async fn list_software(_: AuthUser) -> HttpResponse {
    let software = vec![
        SoftwareInfo {
            name: String::from("nginx"),
            description: String::from("High-performance HTTP server and reverse proxy"),
            versions_available: true,
        },
        SoftwareInfo {
            name: String::from("mysql"),
            description: String::from("MySQL relational database management system"),
            versions_available: true,
        },
        SoftwareInfo {
            name: String::from("pgsql"),
            description: String::from("PostgreSQL object-relational database system"),
            versions_available: true,
        },
        SoftwareInfo {
            name: String::from("redis"),
            description: String::from("Redis in-memory data structure store"),
            versions_available: true,
        },
        SoftwareInfo {
            name: String::from("docker"),
            description: String::from("Docker container runtime"),
            versions_available: false,
        },
    ];

    HttpResponse::Ok().json(ResponseStructure {
        success: true,
        code: 200,
        message: String::from("success"),
        data: Some(software),
    })
}

#[derive(Deserialize)]
pub struct InstallBody {
    pub software: String,
    pub version: Option<String>,
}

pub async fn install_software(_: AuthUser, body: web::Json<InstallBody>) -> HttpResponse {
    let software_type = match body.software.to_lowercase().as_str() {
        "nginx" => SoftwareType::Nginx,
        "mysql" => SoftwareType::Mysql,
        "pgsql" => SoftwareType::Pgsql,
        "redis" => SoftwareType::Redis,
        "docker" => SoftwareType::Docker,
        _ => {
            return HttpResponse::BadRequest().json(ResponseStructureError {
                success: false,
                code: 400,
                message: String::from("Unknown software type"),
            });
        }
    };

    let version = body.version.clone().unwrap_or_else(|| String::from("latest"));

    // Spawn blocking task since installation may take a while
    let result = web::block(move || {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(installer::install(software_type, &version))
    })
    .await;

    match result {
        Ok(Ok(())) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("Installation completed successfully"),
            data: None::<bool>,
        }),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: format!("Installation failed: {}", e),
        }),
        Err(e) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: format!("Installation task failed: {}", e),
        }),
    }
}

#[derive(Deserialize)]
pub struct UninstallBody {
    pub software: String,
}

pub async fn uninstall_software(_: AuthUser, body: web::Json<UninstallBody>) -> HttpResponse {
    let software_type = match body.software.to_lowercase().as_str() {
        "nginx" => SoftwareType::Nginx,
        "mysql" => SoftwareType::Mysql,
        "pgsql" => SoftwareType::Pgsql,
        "redis" => SoftwareType::Redis,
        "docker" => SoftwareType::Docker,
        _ => {
            return HttpResponse::BadRequest().json(ResponseStructureError {
                success: false,
                code: 400,
                message: String::from("Unknown software type"),
            });
        }
    };

    let result = web::block(move || {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(installer::uninstall(software_type))
    })
    .await;

    match result {
        Ok(Ok(())) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("Uninstallation completed successfully"),
            data: None::<bool>,
        }),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: format!("Uninstallation failed: {}", e),
        }),
        Err(e) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: format!("Uninstallation task failed: {}", e),
        }),
    }
}
