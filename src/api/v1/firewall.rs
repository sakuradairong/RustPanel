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
use crate::privileged::{self, Action, AuditedError};
use crate::service::db::DBPool;
use crate::service::rbac::{self, PERM_FIREWALL_READ, PERM_FIREWALL_WRITE};

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

fn runner_error_response(err: privileged::RunnerError) -> HttpResponse {
    if err.is_rejected() {
        ResponseStructureError::forbidden(&err.message())
    } else {
        HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: err.message(),
        })
    }
}

fn audited_error_response(err: AuditedError) -> HttpResponse {
    if err.is_unavailable() {
        HttpResponse::ServiceUnavailable().json(ResponseStructureError {
            success: false,
            code: 503,
            message: err.message(),
        })
    } else if err.is_rejected() {
        ResponseStructureError::forbidden(&err.message())
    } else {
        HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: err.message(),
        })
    }
}

fn result_response(result: privileged::RunnerResult) -> HttpResponse {
    if result.exit_code == Some(0) {
        HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some(serde_json::json!({
                "output": result.stdout,
                "operation_id": result.operation_id,
                "duration_ms": result.duration_ms,
            })),
        })
    } else {
        let message = if result.stderr.trim().is_empty() {
            result.stdout
        } else {
            result.stderr
        };
        HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: message.trim().to_string(),
        })
    }
}

// ── Handlers ──

pub async fn get_firewall_status(user: AuthUser, pool: web::Data<DBPool>) -> HttpResponse {
    if let Err(err) = rbac::require_perm(pool.get_ref(), &user, PERM_FIREWALL_READ) {
        return err.error_response();
    }
    let iptables_available = match privileged::execute(privileged::runner::request_for(
        Some(user.id),
        Action::IptablesList,
    ))
    .await
    {
        Ok(result) => result.exit_code == Some(0),
        Err(_) => false,
    };

    let (ufw_available, ufw_active) =
        match privileged::execute(privileged::runner::request_for(Some(user.id), Action::UfwStatus))
            .await
        {
            Ok(result) if result.exit_code == Some(0) => {
                let active = result.stdout.contains("Status: active")
                    || result.stdout.contains("active");
                (true, Some(active))
            }
            Ok(_) => (true, Some(false)),
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
    match privileged::execute(privileged::runner::request_for(
        Some(user.id),
        Action::IptablesList,
    ))
    .await
    {
        Ok(result) if result.exit_code == Some(0) => {
            let rules = parse_iptables_rules(&result.stdout);
            HttpResponse::Ok().json(ResponseStructure {
                success: true,
                code: 200,
                message: String::from("success"),
                data: Some(rules),
            })
        }
        Ok(result) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: result.stderr.trim().to_string(),
        }),
        Err(err) => runner_error_response(err),
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
    let args = match privileged::validate::tokenize_iptables_rule(&body.rule) {
        Ok(args) => args,
        Err(err) => return runner_error_response(err),
    };
    match privileged::execute_audited(
        pool.get_ref(),
        Some(user.id),
        Action::IptablesAppend {
            chain: body.chain.clone(),
            args,
        },
    )
    .await
    {
        Ok(result) => result_response(result),
        Err(err) => audited_error_response(err),
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
    match privileged::execute_audited(
        pool.get_ref(),
        Some(user.id),
        Action::IptablesDelete {
            chain: path.chain.clone(),
            num: query.num,
        },
    )
    .await
    {
        Ok(result) if result.exit_code == Some(0) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some(serde_json::json!({
                "ok": true,
                "operation_id": result.operation_id,
            })),
        }),
        Ok(result) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: result.stderr.trim().to_string(),
        }),
        Err(err) => audited_error_response(err),
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
    let protocol = body.protocol.clone().unwrap_or_else(|| "tcp".into());
    let jump = body.action.clone().unwrap_or_else(|| "ACCEPT".into());

    match privileged::execute_audited(
        pool.get_ref(),
        Some(user.id),
        Action::IptablesAllowPort {
            port: body.port,
            protocol,
            jump,
        },
    )
    .await
    {
        Ok(result) if result.exit_code == Some(0) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some(serde_json::json!({
                "ok": true,
                "operation_id": result.operation_id,
            })),
        }),
        Ok(result) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false,
            code: 500,
            message: result.stderr.trim().to_string(),
        }),
        Err(err) => audited_error_response(err),
    }
}
