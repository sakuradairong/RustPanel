use std::error::Error;
use std::fmt;
use serde::{Deserialize, Serialize};
use bollard::models::{VolumeCreateRequest, VolumeListResponse};

use super::docker;

#[derive(Debug)]
pub struct DockerVolumeError {
    pub message: String,
}

impl fmt::Display for DockerVolumeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Docker volume error: {}", self.message)
    }
}

impl Error for DockerVolumeError {}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VolumeInfo {
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
    pub created_at: Option<String>,
    pub scope: String,
    pub size: i64,
}

pub async fn list() -> Result<Vec<VolumeInfo>, Box<dyn Error + Send + Sync>> {
    let docker = docker()?;
    let response: VolumeListResponse = docker.list_volumes(None::<bollard::query_parameters::ListVolumesOptions>).await?;

    let volumes: Vec<VolumeInfo> = response.volumes.unwrap_or_default().iter().map(|v| {
        VolumeInfo {
            name: v.name.clone(),
            driver: v.driver.clone(),
            mountpoint: v.mountpoint.clone(),
            created_at: v.created_at.clone(),
            scope: v.scope.clone().map(|s| format!("{:?}", s)).unwrap_or_else(|| String::from("local")),
            size: v.usage_data.as_ref().map(|u| u.size).unwrap_or(0),
        }
    }).collect();

    Ok(volumes)
}

pub async fn create(name: &str, driver: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
    let docker = docker()?;
    let config = VolumeCreateRequest {
        name: Some(name.to_string()),
        driver: Some(driver.to_string()),
        ..Default::default()
    };
    let volume = docker.create_volume(config).await?;
    Ok(volume.name)
}

pub async fn remove(volume_name: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let docker = docker()?;
    docker.remove_volume(volume_name, None::<bollard::query_parameters::RemoveVolumeOptions>).await?;
    Ok(())
}

pub async fn inspect(volume_name: &str) -> Result<VolumeInfo, Box<dyn Error + Send + Sync>> {
    let docker = docker()?;
    let v = docker.inspect_volume(volume_name).await?;
    Ok(VolumeInfo {
        name: v.name.clone(),
        driver: v.driver.clone(),
        mountpoint: v.mountpoint.clone(),
        created_at: v.created_at.clone(),
        scope: v.scope.clone().map(|s| format!("{:?}", s)).unwrap_or_else(|| String::from("local")),
        size: v.usage_data.as_ref().map(|u| u.size).unwrap_or(0),
    })
}
