use actix_web::{web, HttpResponse, ResponseError};
use serde::{Deserialize, Serialize};
use sysinfo::System;

use crate::api::auth::AuthUser;
use crate::api::{ResponseStructure, ResponseStructureError};
use crate::service::db::DBPool;
use crate::service::rbac::{self, PERM_PROCESS_READ, PERM_PROCESS_WRITE};

#[derive(Deserialize)]
pub struct ListProcessesQuery {
    pub keyword: Option<String>,
}

#[derive(Serialize)]
struct ProcessInfo {
    pub pid: i32,
    pub name: String,
    pub cpu: f32,
    pub memory: u64,
    pub status: String,
    pub run_time: u64,
}

#[derive(Deserialize)]
pub struct KillProcessPath {
    pub pid: i32,
}

pub async fn list_processes(user: AuthUser, pool: web::Data<DBPool>, query: web::Query<ListProcessesQuery>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_PROCESS_READ) {
        return err.error_response();
    }
    let keyword = query.keyword.as_ref().map(|k| k.to_lowercase());
    let mut sys = System::new_all();

    // Sleep briefly so per-process CPU usage data can be sampled
    // Sleep briefly so per-process CPU usage data can be sampled
    tokio::time::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL).await;
    sys.refresh_all();

    let mut processes: Vec<ProcessInfo> = Vec::new();

    for (_, process) in sys.processes() {
        let name = process.name().to_string_lossy().to_string();
        let pid = usize::from(process.pid()) as i32;

        // Filter by keyword if provided (match process name or PID substring)
        if let Some(ref kw) = keyword {
            let pid_str = pid.to_string();
            if !name.to_lowercase().contains(kw.as_str()) && !pid_str.contains(kw.as_str()) {
                continue;
            }
        }

        processes.push(ProcessInfo {
            pid,
            name,
            cpu: process.cpu_usage(),
            memory: process.memory(),
            status: process.status().to_string(),
            run_time: process.run_time(),
        });
    }

    HttpResponse::Ok().json(ResponseStructure {
        success: true,
        code: 200,
        message: String::from("success"),
        data: Some(processes),
    })
}

pub async fn kill_process(user: AuthUser, pool: web::Data<DBPool>, path: web::Path<KillProcessPath>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_PROCESS_WRITE) {
        return err.error_response();
    }
    let pid = path.pid;

    match std::process::Command::new("kill")
        .arg("-9")
        .arg(pid.to_string())
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                HttpResponse::Ok().json(ResponseStructure {
                    success: true,
                    code: 200,
                    message: String::from("success"),
                    data: Some(true),
                })
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                HttpResponse::InternalServerError().json(ResponseStructureError {
                    success: false,
                    code: 500,
                    message: format!("Failed to kill process {}: {}", pid, stderr.trim()),
                })
            }
        }
        Err(err) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: format!("Failed to execute kill command: {}", err),
        }),
    }
}
