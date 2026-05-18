use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use sysinfo::System;

use crate::api::auth::AuthUser;
use crate::api::{ResponseStructure, ResponseStructureError};

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

pub async fn list_processes(_: AuthUser, query: web::Query<ListProcessesQuery>) -> HttpResponse {
    let keyword = query.keyword.as_ref().map(|k| k.to_lowercase());
    let mut sys = System::new_all();

    // Sleep briefly so per-process CPU usage data can be sampled
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    sys.refresh_all();

    let mut processes: Vec<ProcessInfo> = Vec::new();

    for (_, process) in sys.processes() {
        let name = process.name().to_string_lossy().to_string();

        // Filter by keyword if provided
        if let Some(ref kw) = keyword {
            if !name.to_lowercase().contains(kw.as_str()) {
                continue;
            }
        }

        processes.push(ProcessInfo {
            pid: usize::from(process.pid()) as i32,
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

pub async fn kill_process(_: AuthUser, path: web::Path<KillProcessPath>) -> HttpResponse {
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
