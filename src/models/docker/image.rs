use bollard::models::ImageSummary;
use bollard::query_parameters::{CreateImageOptions, ListImagesOptions, RemoveImageOptions};
use serde::{Deserialize, Serialize};
use futures_util::StreamExt;
use std::error::Error;

use super::docker;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ImageInfo {
    pub id: String,
    pub repo_tags: Vec<String>,
    pub size: i64,
    pub created: i64,
}

pub async fn list() -> Result<Vec<ImageInfo>, Box<dyn Error + Send + Sync>> {
    let client = docker()?;
    let images = client.list_images(Some(ListImagesOptions {
        all: true,
        ..Default::default()
    })).await?;

    let result = images.into_iter().map(|img: ImageSummary| ImageInfo {
        id: img.id.trim_start_matches("sha256:").to_string(),
        repo_tags: img.repo_tags,
        size: img.size,
        created: img.created,
    }).collect();

    Ok(result)
}

pub async fn pull(image: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
    let client = docker()?;

    let options = CreateImageOptions {
        from_image: Some(image.to_string()),
        ..Default::default()
    };

    let mut stream = client.create_image(Some(options), None, None);
    let mut output = String::new();

    while let Some(item) = stream.next().await {
        match item {
            Ok(info) => {
                if let Some(ref status) = info.status {
                    output.push_str(status);
                    output.push('\n');
                }
                if let Some(error_detail) = info.error_detail {
                    let msg = format!("[{}] {}", error_detail.code.unwrap_or(0), error_detail.message.unwrap_or_default());
                    return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, msg)));
                }
            }
            Err(e) => return Err(Box::new(e)),
        }
    }

    Ok(output)
}

pub async fn remove(image_id: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let client = docker()?;
    let options = RemoveImageOptions { force: true, ..Default::default() };
    client.remove_image(image_id, Some(options), None).await?;
    Ok(())
}
