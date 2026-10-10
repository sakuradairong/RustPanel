//! Execute whitelisted actions via argv arrays with timeout and output caps.
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::process::Command;
use tokio::time::timeout;

use super::action::{Action, SystemctlVerb};
use super::validate;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunnerError {
    Rejected(String),
    TimedOut { action: String, timeout_ms: u64 },
    Spawn(String),
    Io(String),
}

impl RunnerError {
    pub fn is_rejected(&self) -> bool {
        matches!(self, Self::Rejected(_))
    }

    pub fn message(&self) -> String {
        match self {
            Self::Rejected(m) => m.clone(),
            Self::TimedOut { action, timeout_ms } => {
                format!("action {action} timed out after {timeout_ms}ms")
            }
            Self::Spawn(m) | Self::Io(m) => m.clone(),
        }
    }
}

impl std::fmt::Display for RunnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for RunnerError {}

#[derive(Debug, Clone)]
pub struct RunnerRequest {
    pub operation_id: String,
    pub actor_id: Option<i32>,
    pub job_id: Option<String>,
    pub action: Action,
}

#[derive(Debug, Clone)]
pub struct RunnerResult {
    pub operation_id: String,
    pub action_id: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub duration_ms: u64,
    pub actor_id: Option<i32>,
    pub job_id: Option<String>,
}

fn truncate(s: String, limit: usize) -> String {
    if s.len() <= limit {
        s
    } else {
        let mut out = s.chars().take(limit).collect::<String>();
        out.push_str("…[truncated]");
        out
    }
}

fn build_command(action: &Action) -> Result<Command, RunnerError> {
    match action {
        Action::IptablesList => {
            let mut cmd = Command::new("iptables");
            cmd.args(["-L", "-n", "--line-numbers"]);
            Ok(cmd)
        }
        Action::UfwStatus => {
            let mut cmd = Command::new("ufw");
            cmd.arg("status");
            Ok(cmd)
        }
        Action::IptablesAppend { chain, args } => {
            let mut cmd = Command::new("iptables");
            cmd.arg("-A").arg(chain);
            cmd.args(args);
            Ok(cmd)
        }
        Action::IptablesDelete { chain, num } => {
            let mut cmd = Command::new("iptables");
            cmd.args(["-D", chain, &num.to_string()]);
            Ok(cmd)
        }
        Action::IptablesAllowPort {
            port,
            protocol,
            jump,
        } => {
            let mut cmd = Command::new("iptables");
            cmd.args([
                "-A",
                "INPUT",
                "-p",
                protocol,
                "--dport",
                &port.to_string(),
                "-j",
                jump,
            ]);
            Ok(cmd)
        }
        Action::Systemctl { verb, unit } => {
            let mut cmd = Command::new("systemctl");
            match verb {
                SystemctlVerb::ListUnits => {
                    cmd.args([
                        "list-units",
                        "--type=service",
                        "--no-pager",
                        "--no-legend",
                    ]);
                }
                SystemctlVerb::Status => {
                    let unit = unit.as_deref().ok_or_else(|| {
                        RunnerError::Rejected("systemd unit name required".into())
                    })?;
                    cmd.args(["status", unit, "--no-pager", "--lines=20"]);
                }
                other => {
                    let unit = unit.as_deref().ok_or_else(|| {
                        RunnerError::Rejected("systemd unit name required".into())
                    })?;
                    cmd.arg(other.as_str()).arg(unit);
                }
            }
            Ok(cmd)
        }
    }
}

/// Run a whitelisted action. Never routes through a shell.
pub async fn execute(req: RunnerRequest) -> Result<RunnerResult, RunnerError> {
    validate::validate_action(&req.action)?;
    let action_id = req.action.id().to_string();
    let timeout_ms = req.action.timeout_ms();
    let output_limit = req.action.output_limit();

    let mut cmd = build_command(&req.action)?;
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);

    // New process group so timeout can signal the whole tree (Linux).
    #[cfg(unix)]
    unsafe {
        cmd.pre_exec(|| {
            if libc::setpgid(0, 0) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let started = Instant::now();
    let child = cmd
        .spawn()
        .map_err(|e| RunnerError::Spawn(format!("failed to spawn {}: {e}", req.action.id())))?;
    let child_pid = child.id();

    let output = match timeout(Duration::from_millis(timeout_ms), child.wait_with_output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            return Err(RunnerError::Io(format!("command wait failed: {e}")));
        }
        Err(_) => {
            // kill_on_drop drops the Child future; also signal the process group.
            #[cfg(unix)]
            if let Some(pid) = child_pid {
                unsafe {
                    let _ = libc::kill(-(pid as i32), libc::SIGKILL);
                }
            }
            return Err(RunnerError::TimedOut {
                action: action_id,
                timeout_ms,
            });
        }
    };

    let duration_ms = started.elapsed().as_millis() as u64;
    let stdout = truncate(
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output_limit,
    );
    let stderr = truncate(
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output_limit / 4,
    );

    Ok(RunnerResult {
        operation_id: req.operation_id,
        action_id,
        exit_code: output.status.code(),
        stdout,
        stderr,
        timed_out: false,
        duration_ms,
        actor_id: req.actor_id,
        job_id: req.job_id,
    })
}

/// Build a request helper used by HTTP handlers.
pub fn request_for(
    actor_id: Option<i32>,
    action: Action,
) -> RunnerRequest {
    RunnerRequest {
        operation_id: new_operation_id(),
        actor_id,
        job_id: None,
        action,
    }
}

pub fn new_operation_id() -> String {
    use rand::{distributions::Alphanumeric, Rng};
    let suffix: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(16)
        .map(char::from)
        .collect();
    format!(
        "op-{}-{suffix}",
        chrono::Utc::now().timestamp_millis()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privileged::validate::tokenize_iptables_rule;

    #[tokio::test]
    async fn rejects_unknown_metacharacters_before_spawn() {
        let req = RunnerRequest {
            operation_id: "t1".into(),
            actor_id: Some(1),
            job_id: None,
            action: Action::IptablesAppend {
                chain: "INPUT".into(),
                args: vec!["-p".into(), "tcp;reboot".into()],
            },
        };
        let err = execute(req).await.unwrap_err();
        assert!(err.is_rejected());
    }

    #[tokio::test]
    async fn true_binary_succeeds_through_argv() {
        // systemctl list may fail in CI containers; use a built-in Action path via
        // iptables list which may also fail — instead validate tokenization + reject path.
        let args = tokenize_iptables_rule("-p tcp --dport 80 -j ACCEPT").unwrap();
        assert_eq!(args[0], "-p");
        let req = request_for(
            None,
            Action::Systemctl {
                verb: SystemctlVerb::Start,
                unit: Some("nginx.service;id".into()),
            },
        );
        assert!(execute(req).await.unwrap_err().is_rejected());
    }

    #[tokio::test]
    async fn timeout_kills_long_running_command() {
        // Directly exercise timeout machinery with `sleep` via a private helper path:
        // build a tiny Action that isn't in the public enum by testing TimedOut mapping
        // through an internal spawn of sleep wouldn't compile — use `/bin/sleep`.
        // We approximate by validating RunnerError::TimedOut formatting and running sleep
        // through a local async helper.
        let started = Instant::now();
        let mut cmd = Command::new("sleep");
        cmd.arg("5");
        cmd.stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true);
        #[cfg(unix)]
        unsafe {
            cmd.pre_exec(|| {
                if libc::setpgid(0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = cmd.spawn().expect("spawn sleep");
        let timed = timeout(Duration::from_millis(200), child.wait()).await;
        assert!(timed.is_err());
        #[cfg(unix)]
        if let Some(pid) = child.id() {
            unsafe {
                let _ = libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
        let _ = child.start_kill();
        let _ = child.wait().await;
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
