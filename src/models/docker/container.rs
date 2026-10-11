use std::collections::HashMap;
use std::error::Error;
use std::fmt;

use bollard::container::LogOutput;
use bollard::models::{
    ContainerCreateBody, HostConfig, PortBinding, PortSummaryTypeEnum, RestartPolicy,
    RestartPolicyNameEnum,
};
use bollard::query_parameters::{
    CreateContainerOptions, ListContainersOptions, LogsOptions, RemoveContainerOptions,
    RestartContainerOptions, StartContainerOptions, StatsOptions, StopContainerOptions,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

use super::docker;

#[derive(Debug)]
pub struct DockerContainerError {
    pub message: String,
}

impl fmt::Display for DockerContainerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for DockerContainerError {}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub state: String,
    pub created: i64,
    pub ports: Vec<String>,
}

pub async fn list(all: bool) -> Result<Vec<ContainerInfo>, Box<dyn Error + Send + Sync>> {
    let client = docker()?;
    let options = ListContainersOptions {
        all,
        ..Default::default()
    };

    let containers = client.list_containers(Some(options)).await?;

    let mut result = Vec::new();
    for c in containers {
        let names = c.names.unwrap_or_default();
        let name = names
            .first()
            .map(|n| n.trim_start_matches('/').to_string())
            .unwrap_or_default();

        let ports = c
            .ports
            .unwrap_or_default()
            .iter()
            .map(|p| {
                let typ_str = match p.typ.as_ref() {
                    Some(PortSummaryTypeEnum::TCP) => "tcp",
                    Some(PortSummaryTypeEnum::UDP) => "udp",
                    Some(PortSummaryTypeEnum::SCTP) => "sctp",
                    _ => "tcp",
                };
                format!(
                    "{}:{}->{}/{}",
                    p.ip.as_deref().unwrap_or("0.0.0.0"),
                    p.public_port.unwrap_or(0),
                    p.private_port,
                    typ_str
                )
            })
            .collect::<Vec<_>>();

        // Normalize to lowercase so the UI can compare against "running"/"paused"/etc.
        let state_str = match c.state.as_ref() {
            Some(s) => format!("{:?}", s).to_lowercase(),
            None => String::new(),
        };

        result.push(ContainerInfo {
            id: c.id.unwrap_or_default(),
            name,
            image: c.image.unwrap_or_default(),
            status: c.status.unwrap_or_default(),
            state: state_str,
            created: c.created.unwrap_or(0),
            ports,
        });
    }

    Ok(result)
}

pub async fn create(
    name: &str,
    image: &str,
    cmd: Option<Vec<String>>,
    ports: Vec<String>,
    env: Vec<String>,
    restart_policy: Option<String>,
    network_mode: Option<String>,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let client = docker()?;

    // Parse port mappings like "8080:80" or "8080:80/udp" (host:container[/proto]).
    // Docker auto-exposes ports that appear in HostConfig.port_bindings.
    let mut port_bindings: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();
    for p in &ports {
        let (mapping, proto) = match p.split_once('/') {
            Some((m, pr)) => (m.trim(), pr.trim()),
            None => (p.trim(), "tcp"),
        };
        if mapping.is_empty() {
            continue;
        }
        let (host_port, container_port) = match mapping.split_once(':') {
            Some((h, c)) => (h.trim().to_string(), c.trim().to_string()),
            None => (mapping.to_string(), mapping.to_string()),
        };
        if container_port.is_empty() {
            continue;
        }
        let key = format!("{}/{}", container_port, proto);
        port_bindings.insert(
            key,
            Some(vec![PortBinding {
                host_ip: Some("0.0.0.0".to_string()),
                host_port: Some(host_port),
            }]),
        );
    }

    let restart = match restart_policy.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some("always") => Some(RestartPolicy {
            name: Some(RestartPolicyNameEnum::ALWAYS),
            maximum_retry_count: None,
        }),
        Some("unless-stopped") => Some(RestartPolicy {
            name: Some(RestartPolicyNameEnum::UNLESS_STOPPED),
            maximum_retry_count: None,
        }),
        Some("on-failure") => Some(RestartPolicy {
            name: Some(RestartPolicyNameEnum::ON_FAILURE),
            maximum_retry_count: Some(5),
        }),
        Some("no") => Some(RestartPolicy {
            name: Some(RestartPolicyNameEnum::NO),
            maximum_retry_count: None,
        }),
        _ => None,
    };
    let network = network_mode
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let host_config = if port_bindings.is_empty() && restart.is_none() && network.is_none() {
        None
    } else {
        Some(HostConfig {
            port_bindings: if port_bindings.is_empty() {
                None
            } else {
                Some(port_bindings)
            },
            restart_policy: restart,
            network_mode: network,
            ..Default::default()
        })
    };

    let config = ContainerCreateBody {
        image: Some(image.to_string()),
        cmd: cmd.clone(),
        tty: Some(true),
        env: if env.is_empty() { None } else { Some(env) },
        host_config,
        ..Default::default()
    };

    let options = CreateContainerOptions {
        name: Some(name.to_string()),
        ..Default::default()
    };

    let response = client.create_container(Some(options), config).await?;
    Ok(response.id)
}

pub async fn start(container_id: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let client = docker()?;
    client.start_container(container_id, None::<StartContainerOptions>).await?;
    Ok(())
}

pub async fn stop(container_id: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let client = docker()?;
    let options = StopContainerOptions { t: Some(10), signal: None };
    client.stop_container(container_id, Some(options)).await?;
    Ok(())
}

pub async fn remove(container_id: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let client = docker()?;
    let options = RemoveContainerOptions { force: true, v: true, ..Default::default() };
    client.remove_container(container_id, Some(options)).await?;
    Ok(())
}

pub async fn restart(container_id: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let client = docker()?;
    let options = RestartContainerOptions { t: Some(10), signal: None };
    client.restart_container(container_id, Some(options)).await?;
    Ok(())
}

pub async fn pause(container_id: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let client = docker()?;
    client.pause_container(container_id).await?;
    Ok(())
}

pub async fn unpause(container_id: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let client = docker()?;
    client.unpause_container(container_id).await?;
    Ok(())
}

const EXEC_OUTPUT_LIMIT: usize = 1024 * 1024;
const EXEC_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Debug, Serialize)]
pub struct ExecResult {
    pub output: String,
    pub exit_code: Option<i64>,
    pub truncated: bool,
    pub timed_out: bool,
    pub exec_id: String,
}

async fn collect_exec_output<S>(
    mut stream: S,
    output: &mut Vec<u8>,
    truncated: &mut bool,
) -> Result<(), bollard::errors::Error>
where
    S: futures_util::Stream<Item = Result<LogOutput, bollard::errors::Error>> + Unpin,
{
    while let Some(item) = stream.next().await {
        let message = match item? {
            LogOutput::StdOut { message }
            | LogOutput::StdErr { message }
            | LogOutput::Console { message } => message,
            LogOutput::StdIn { .. } => continue,
        };
        let remaining = EXEC_OUTPUT_LIMIT.saturating_sub(output.len());
        *truncated |= message.len() > remaining;
        output.extend_from_slice(&message[..message.len().min(remaining)]);
    }
    Ok(())
}

/// Drain output with bounded memory and an overall deadline, then inspect the
/// command's exit status. Dropping the attachment on timeout does not kill a
/// Docker exec process: report that uncertainty and never retry it automatically.
pub async fn exec(
    container_id: &str,
    cmd: Vec<String>,
) -> Result<ExecResult, Box<dyn Error + Send + Sync>> {
    use bollard::exec::{CreateExecOptions, StartExecOptions, StartExecResults};
    if cmd.is_empty() {
        return Err(Box::new(DockerContainerError {
            message: "command is required".into(),
        }));
    }
    let client = docker()?;
    let deadline = tokio::time::Instant::now() + EXEC_TIMEOUT;
    let created = tokio::time::timeout_at(
        deadline,
        client.create_exec(
            container_id,
            CreateExecOptions {
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                cmd: Some(cmd),
                ..Default::default()
            },
        ),
    )
    .await
    .map_err(|_| DockerContainerError {
        message: "Docker exec creation timed out".into(),
    })??;
    let mut output = Vec::new();
    let mut truncated = false;
    let completed = tokio::time::timeout_at(deadline, async {
        match client
            .start_exec(&created.id, None::<StartExecOptions>)
            .await?
        {
            StartExecResults::Attached { output: stream, .. } => {
                collect_exec_output(stream, &mut output, &mut truncated).await?
            }
            StartExecResults::Detached => {
                return Err(bollard::errors::Error::IOError {
                    err: std::io::Error::new(std::io::ErrorKind::Other, "Unexpected detached exec"),
                })
            }
        }
        let inspected = client.inspect_exec(&created.id).await?;
        Ok::<_, bollard::errors::Error>(inspected.exit_code)
    })
    .await;
    let (exit_code, timed_out) = match completed {
        Ok(result) => (result?, false),
        Err(_) => (None, true),
    };
    Ok(ExecResult {
        output: String::from_utf8_lossy(&output).into_owned(),
        exit_code,
        truncated,
        timed_out,
        exec_id: created.id,
    })
}

/// Start an interactive TTY exec session (stdin + stdout). Caller bridges I/O.
pub async fn exec_interactive(
    container_id: &str,
    cmd: Vec<String>,
) -> Result<bollard::exec::StartExecResults, Box<dyn Error + Send + Sync>> {
    use bollard::exec::{CreateExecOptions, StartExecOptions};

    if cmd.is_empty() {
        return Err(Box::new(DockerContainerError {
            message: "command is required".into(),
        }));
    }

    let client = docker()?;
    let created = client
        .create_exec(
            container_id,
            CreateExecOptions {
                attach_stdin: Some(true),
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                tty: Some(true),
                cmd: Some(cmd),
                ..Default::default()
            },
        )
        .await?;

    let results = client
        .start_exec(
            &created.id,
            Some(StartExecOptions {
                detach: false,
                tty: true,
                output_capacity: Some(1024 * 64),
            }),
        )
        .await?;

    Ok(results)
}

pub async fn logs(
    container_id: &str,
    tail: usize,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let client = docker()?;
    let options = LogsOptions {
        stdout: true,
        stderr: true,
        tail: tail.to_string(),
        ..Default::default()
    };

    let mut stream = client.logs(container_id, Some(options));
    let mut output = String::new();

    while let Some(item) = stream.next().await {
        match item {
            Ok(LogOutput::StdOut { message }) | Ok(LogOutput::StdErr { message }) => {
                output.push_str(&String::from_utf8_lossy(&message));
            }
            Ok(LogOutput::StdIn { .. }) | Ok(LogOutput::Console { .. }) => {}
            Err(e) => output.push_str(&format!("[Docker error: {}]", e)),
        }
    }

    Ok(output)
}

pub async fn inspect(container_id: &str) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
    let client = docker()?;
    let info = client.inspect_container(container_id, None).await?;
    Ok(serde_json::to_value(info)?)
}

pub async fn stats(container_id: &str) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
    let client = docker()?;
    let options = StatsOptions { stream: false, one_shot: true };
    let mut stream = client.stats(container_id, Some(options));
    if let Some(item) = stream.next().await {
        match item {
            Ok(stats) => return Ok(serde_json::to_value(stats)?),
            Err(e) => return Err(Box::new(e)),
        }
    }
    Err(Box::new(std::io::Error::new(std::io::ErrorKind::NotFound, "no stats data")))
}

#[cfg(test)]
mod exec_tests {
    use super::*;
    #[tokio::test]
    async fn output_is_bounded_and_consumes_all_stdout_and_stderr() {
        let chunks = vec![
            Ok(LogOutput::StdOut {
                message: vec![b'a'; EXEC_OUTPUT_LIMIT].into(),
            }),
            Ok(LogOutput::StdErr {
                message: b"beyond limit".to_vec().into(),
            }),
        ];
        let mut output = Vec::new();
        let mut truncated = false;
        collect_exec_output(
            futures_util::stream::iter(chunks),
            &mut output,
            &mut truncated,
        )
        .await
        .unwrap();
        assert_eq!(output.len(), EXEC_OUTPUT_LIMIT);
        assert!(truncated);
    }

    #[tokio::test]
    async fn stalled_output_can_be_cancelled_at_the_deadline() {
        let mut output = Vec::new();
        let mut truncated = false;
        let stream = futures_util::stream::pending::<Result<LogOutput, bollard::errors::Error>>();
        assert!(tokio::time::timeout(
            std::time::Duration::from_millis(10),
            collect_exec_output(stream, &mut output, &mut truncated)
        )
        .await
        .is_err());
    }
}
