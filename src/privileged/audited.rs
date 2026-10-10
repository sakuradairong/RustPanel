//! Run privileged actions only after a durable audit intent is recorded.
use serde_json::{json, Value};

use crate::audit::{self, IntentSpec, Outcome};
use crate::service::db::DBPool;

use super::action::Action;
use super::runner::{self, RunnerError, RunnerRequest, RunnerResult};

#[derive(Debug)]
pub enum AuditedError {
    Audit(audit::AuditError),
    Runner(RunnerError),
}

impl AuditedError {
    pub fn message(&self) -> String {
        match self {
            Self::Audit(e) => e.message().to_string(),
            Self::Runner(e) => e.message(),
        }
    }

    pub fn is_rejected(&self) -> bool {
        match self {
            Self::Audit(audit::AuditError::Rejected(_)) => true,
            Self::Audit(audit::AuditError::Unavailable(_)) => false,
            Self::Runner(e) => e.is_rejected(),
        }
    }

    pub fn is_unavailable(&self) -> bool {
        matches!(self, Self::Audit(audit::AuditError::Unavailable(_)))
    }
}

fn action_params(action: &Action) -> Value {
    match action {
        Action::IptablesList => json!({}),
        Action::UfwStatus => json!({}),
        Action::IptablesAppend { chain, args } => json!({"chain": chain, "args": args}),
        Action::IptablesDelete { chain, num } => json!({"chain": chain, "num": num}),
        Action::IptablesAllowPort {
            port,
            protocol,
            jump,
        } => json!({"port": port, "protocol": protocol, "jump": jump}),
        Action::Systemctl { verb, unit } => json!({"verb": verb.as_str(), "unit": unit}),
    }
}

fn resource_for(action: &Action) -> Option<String> {
    match action {
        Action::IptablesList
        | Action::UfwStatus
        | Action::IptablesAppend { .. }
        | Action::IptablesDelete { .. }
        | Action::IptablesAllowPort { .. } => Some("firewall".into()),
        Action::Systemctl { unit, .. } => Some(format!("service:{}", unit.as_deref().unwrap_or("*"))),
    }
}

/// Persist intent → execute → record outcome/outbox. Intent failure refuses execution.
pub async fn execute_audited(
    pool: &DBPool,
    actor_id: Option<i32>,
    action: Action,
) -> Result<RunnerResult, AuditedError> {
    let req = RunnerRequest {
        operation_id: runner::new_operation_id(),
        actor_id,
        job_id: None,
        action: action.clone(),
    };
    let intent = audit::begin_intent(
        pool,
        IntentSpec {
            operation_id: req.operation_id.clone(),
            actor_id,
            request_id: None,
            action: action.id().to_string(),
            resource: resource_for(&action),
            params: action_params(&action),
            ip: None,
        },
    )
    .map_err(AuditedError::Audit)?;

    match runner::execute(req).await {
        Ok(result) => {
            let outcome = if result.exit_code == Some(0) {
                Outcome::Success
            } else {
                Outcome::Failure
            };
            if let Err(audit_err) = audit::complete_intent(
                pool,
                &intent,
                outcome,
                result.exit_code,
                json!({
                    "stdout": result.stdout,
                    "stderr": result.stderr,
                }),
                Some(result.duration_ms as i64),
            ) {
                // The action already ran; report loudly instead of dropping
                // the outcome. The stale-intent reconciliation will mark the
                // intent aborted if this write truly failed.
                crate::log_error!(
                    target: "audit",
                    "audit completion failed for {}: {}",
                    intent.operation_id,
                    audit_err.message()
                );
            }
            Ok(result)
        }
        Err(RunnerError::TimedOut { action, timeout_ms }) => {
            if let Err(audit_err) = audit::complete_intent(
                pool,
                &intent,
                Outcome::Timeout,
                None,
                json!({"timeout_ms": timeout_ms}),
                Some(timeout_ms as i64),
            ) {
                crate::log_error!(
                    target: "audit",
                    "audit completion failed for {}: {}",
                    intent.operation_id,
                    audit_err.message()
                );
            }
            Err(AuditedError::Runner(RunnerError::TimedOut {
                action,
                timeout_ms,
            }))
        }
        Err(err) => {
            let outcome = if err.is_rejected() {
                Outcome::Rejected
            } else {
                Outcome::Failure
            };
            if let Err(audit_err) = audit::complete_intent(
                pool,
                &intent,
                outcome,
                None,
                json!({"error": err.message()}),
                None,
            ) {
                crate::log_error!(
                    target: "audit",
                    "audit completion failed for {}: {}",
                    intent.operation_id,
                    audit_err.message()
                );
            }
            Err(AuditedError::Runner(err))
        }
    }
}
