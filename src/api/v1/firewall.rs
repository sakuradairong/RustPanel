/*
 * @Descripttion:
 * @version:
 * @Author: Wynters
 * @Date: 2024-05-25 16:23:02
 * @LastEditTime: 2024-05-25 16:23:02
 * @FilePath: \RustPanel\src\api\v1\firewall.rs
 */

use actix_web::{web, HttpResponse, ResponseError};
use serde::{Deserialize, Serialize};
use crate::api::auth::AuthUser;
use crate::api::{ResponseStructure, ResponseStructureError};
use crate::service::db::DBPool;
use crate::service::rbac::{self, PERM_FIREWALL_READ, PERM_FIREWALL_WRITE};
use tokio::process::Command;

// ── Response types ──

#[derive(Serialize)]
struct FirewallStatus {
    iptables_available: bool,
    ufw_available: bool,
    ufw_active: Option<bool>,
}

#[derive(Serialize)]
struct IptablesRule {
    chain: String,
    num: i32,
    target: String,
    prot: String,
    source: String,
    destination: String,
    extra: String,
}

// ── Request types ──

#[derive(Deserialize)]
pub struct AddRuleBody {
    pub chain: String,
    pub rule: String,
}

#[derive(Deserialize)]
pub struct DeleteRulePath {
    pub chain: String,
}

#[derive(Deserialize)]
pub struct DeleteRuleQuery {
    pub num: i32,
}

#[derive(Deserialize)]
pub struct PortActionBody {
    pub port: u16,
    #[serde(default)]
    pub protocol: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
}

// ── Helpers ──

fn parse_iptables_rules(output: &str) -> Vec<IptablesRule> {
    let mut rules = Vec::new();
    let mut current_chain = String::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Chain ") {
            // Chain INPUT (policy ACCEPT) -> "INPUT"
            current_chain = trimmed
                .strip_prefix("Chain ")
                .and_then(|s| s.split(' ').next())
                .map(|s| s.to_string())
                .unwrap_or_default();
        } else if let Some(c) = trimmed.chars().next() {
            if c.is_ascii_digit() {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 6 {
                    let num: i32 = parts[0].parse().unwrap_or(0);
                    let extra = if parts.len() > 6 {
                        parts[6..].join(" ")
                    } else {
                        String::new()
                    };
                    rules.push(IptablesRule {
                        chain: current_chain.clone(),
                        num,
                        target: parts[1].to_string(),
                        prot: parts[2].to_string(),
                        source: parts[4].to_string(),
                        destination: parts[5].to_string(),
                        extra,
                    });
                }
            }
        }
    }

    rules
}


fn output_response(out: std::process::Output) -> HttpResponse {
    if out.status.success() {
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some(serde_json::json!({"output": stdout})),
        })
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: stderr.trim().to_string(),
        })
    }
}

// ── Handlers ──

pub async fn get_firewall_status(user: AuthUser, pool: web::Data<DBPool>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_FIREWALL_READ) {
        return err.error_response();
    }
    let iptables_available = match Command::new("iptables")
        .args(["-L", "-n", "--line-numbers"])
        .output()
        .await
    {
        Ok(output) => output.status.success(),
        Err(_) => false,
    };

    let (ufw_available, ufw_active) = match Command::new("ufw")
        .arg("status")
        .output()
        .await
    {
        Ok(output) => {
            if output.status.success() {
                let status_str = String::from_utf8_lossy(&output.stdout);
                let active = status_str.contains("Status: active") || status_str.contains("active");
                (true, Some(active))
            } else {
                (true, Some(false))
            }
        }
        Err(_) => (false, None),
    };

    HttpResponse::Ok().json(ResponseStructure {
        success: true,
        code: 200,
        message: String::from("success"),
        data: Some(FirewallStatus {
            iptables_available,
            ufw_available,
            ufw_active,
        }),
    })
}

pub async fn list_iptables_rules(user: AuthUser, pool: web::Data<DBPool>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_FIREWALL_READ) {
        return err.error_response();
    }
    match Command::new("iptables")
        .args(["-L", "-n", "--line-numbers"])
        .output()
        .await
    {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let rules = parse_iptables_rules(&stdout);
                HttpResponse::Ok().json(ResponseStructure {
                    success: true,
                    code: 200,
                    message: String::from("success"),
                    data: Some(rules),
                })
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                HttpResponse::InternalServerError().json(ResponseStructureError {
                    success: false,
                    code: 500,
                    message: stderr.trim().to_string(),
                })
            }
        }
        Err(e) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: format!("Failed to execute iptables: {}", e),
        }),
    }
}

pub async fn add_iptables_rule(
    user: AuthUser,
    pool: web::Data<DBPool>,
    body: web::Json<AddRuleBody>,
) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_FIREWALL_WRITE) {
        return err.error_response();
    }
    let mut cmd = Command::new("iptables");
    cmd.arg("-A").arg(&body.chain);
    for part in body.rule.split_whitespace() {
        cmd.arg(part);
    }

    match cmd.output().await {
        Ok(out) => output_response(out),
        Err(e) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: format!("Failed to execute iptables: {}", e),
        }),
    }
}

pub async fn delete_iptables_rule(
    user: AuthUser,
    pool: web::Data<DBPool>,
    path: web::Path<DeleteRulePath>,
    query: web::Query<DeleteRuleQuery>,
) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_FIREWALL_WRITE) {
        return err.error_response();
    }
    match Command::new("iptables")
        .args(["-D", &path.chain, &query.num.to_string()])
        .output()
        .await
    {
        Ok(out) => {
            if out.status.success() {
                HttpResponse::Ok().json(ResponseStructure {
                    success: true,
                    code: 200,
                    message: String::from("success"),
                    data: Some("ok"),
                })
            } else {
                let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                HttpResponse::InternalServerError().json(ResponseStructureError {
                    success: false,
                    code: 500,
                    message: stderr.trim().to_string(),
                })
            }
        }
        Err(e) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: format!("Failed to execute iptables: {}", e),
        }),
    }
}

pub async fn enable_port(
    user: AuthUser,
    pool: web::Data<DBPool>,
    body: web::Json<PortActionBody>,
) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_FIREWALL_WRITE) {
        return err.error_response();
    }
    let protocol = body.protocol.as_deref().unwrap_or("tcp");
    let action = body.action.as_deref().unwrap_or("ACCEPT");

    match Command::new("iptables")
        .args(["-A", "INPUT", "-p", protocol, "--dport"])
        .arg(body.port.to_string())
        .args(["-j", action])
        .output()
        .await
    {
        Ok(out) => {
            if out.status.success() {
                HttpResponse::Ok().json(ResponseStructure {
                    success: true,
                    code: 200,
                    message: String::from("success"),
                    data: Some("ok"),
                })
            } else {
                let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                HttpResponse::InternalServerError().json(ResponseStructureError {
                    success: false,
                    code: 500,
                    message: stderr.trim().to_string(),
                })
            }
        }
        Err(e) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: format!("Failed to execute iptables: {}", e),
        }),
    }
}
