/*
 * @Descripttion: Docker network management model
 * @version:
 * @Author: Wynters
 * @Date: 2024-07-11 04:20:17
 * @LastEditTime: 2025-12-15 13:17:54
 * @FilePath: \RustPanel\src\models\docker\network.rs
 */

use std::error::Error;
use std::fmt;

use bollard::models::{NetworkConnectRequest, NetworkCreateRequest, NetworkDisconnectRequest};
use bollard::query_parameters::ListNetworksOptions;
use serde::{Deserialize, Serialize};

use super::docker;

#[derive(Debug)]
pub struct DockerNetworkError {
    pub message: String,
}

impl fmt::Display for DockerNetworkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for DockerNetworkError {}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NetworkInfo {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub scope: String,
    pub subnet: String,
    pub gateway: String,
}

pub async fn list() -> Result<Vec<NetworkInfo>, Box<dyn Error + Send + Sync>> {
    let client = docker()?;
    let options = ListNetworksOptions {
        ..Default::default()
    };

    let networks = client.list_networks(Some(options)).await?;

    let mut result = Vec::new();
    for net in networks {
        let (subnet, gateway) = net
            .ipam
            .as_ref()
            .and_then(|ipam| ipam.config.as_ref())
            .and_then(|configs| configs.first())
            .map(|cfg| {
                (
                    cfg.subnet.clone().unwrap_or_default(),
                    cfg.gateway.clone().unwrap_or_default(),
                )
            })
            .unwrap_or_default();

        result.push(NetworkInfo {
            id: net.id.unwrap_or_default(),
            name: net.name.unwrap_or_default(),
            driver: net.driver.unwrap_or_default(),
            scope: net.scope.unwrap_or_default(),
            subnet,
            gateway,
        });
    }

    Ok(result)
}

pub async fn create(
    name: &str,
    driver: &str,
    subnet: Option<&str>,
    gateway: Option<&str>,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let client = docker()?;

    let mut config = NetworkCreateRequest {
        name: name.to_string(),
        driver: Some(driver.to_string()),
        ..Default::default()
    };

    if subnet.is_some() || gateway.is_some() {
        let ipam_config = bollard::models::IpamConfig {
            subnet: subnet.map(|s| s.to_string()),
            gateway: gateway.map(|g| g.to_string()),
            ..Default::default()
        };
        config.ipam = Some(bollard::models::Ipam {
            config: Some(vec![ipam_config]),
            ..Default::default()
        });
    }

    let response = client.create_network(config).await?;
    Ok(response.id)
}

pub async fn remove(network_id: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let client = docker()?;
    client.remove_network(network_id).await?;
    Ok(())
}

pub async fn connect(
    network_id: &str,
    container_id: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let client = docker()?;
    let config = NetworkConnectRequest {
        container: container_id.to_string(),
        ..Default::default()
    };
    client.connect_network(network_id, config).await?;
    Ok(())
}

pub async fn disconnect(
    network_id: &str,
    container_id: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let client = docker()?;
    let config = NetworkDisconnectRequest {
        container: container_id.to_string(),
        force: Some(false),
    };
    client.disconnect_network(network_id, config).await?;
    Ok(())
}
