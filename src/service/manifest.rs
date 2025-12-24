use std::sync::Arc;
use tokio::{process::Command, sync::Mutex};

use thiserror::Error;

pub type Result<T> = std::result::Result<T, ImageWatcherError>;

#[derive(Error, Debug)]
pub enum ImageWatcherError {
    #[error("Digest string '{0}' does not start with 'sha256:'")]
    InvalidDigestPrefix(String),
    #[error("JSON deserialization error: {0}")]
    SerdeJsonError(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Docker command failed: {0}")]
    DockerCommandFailed(String),
}

#[derive(serde::Deserialize)]
pub struct DockerInspectResponse {
    #[serde(rename = "RepoDigests")]
    repo_digests: Option<Vec<String>>,
    #[serde(rename = "Id")]
    id: Option<String>,
}

impl DockerInspectResponse {
    pub fn get_digest(&self) -> Result<Sha256> {
        // Try to get digest from RepoDigests first
        if let Some(digests) = self.repo_digests.as_ref() {
            if let Some(first_digest) = digests.first() {
                // RepoDigests format is like "repository@sha256:..."
                if let Some(digest_part) = first_digest.split('@').nth(1) {
                    let hash = digest_part.strip_prefix("sha256:").ok_or_else(|| {
                        ImageWatcherError::InvalidDigestPrefix(digest_part.to_string())
                    })?;
                    let mut inner = [0u8; 64];
                    inner.copy_from_slice(hash.as_bytes());
                    return Ok(Sha256 { inner });
                }
            }
        }

        // Fallback to Id if RepoDigests is not available
        if let Some(id) = self.id.as_ref() {
            let hash = id
                .strip_prefix("sha256:")
                .ok_or_else(|| ImageWatcherError::InvalidDigestPrefix(id.clone()))?;
            let mut inner = [0u8; 64];
            inner.copy_from_slice(hash.as_bytes());
            return Ok(Sha256 { inner });
        }

        Err(ImageWatcherError::DockerCommandFailed(
            "No digest found in inspect output".to_string(),
        ))
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Sha256 {
    /// 256 bits of data in base64
    pub inner: [u8; 64],
}

impl std::fmt::Debug for Sha256 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let hash_str = std::str::from_utf8(&self.inner).unwrap_or("<invalid utf8>");
        write!(f, "Sha256(sha256:{})", hash_str)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageWatcher {
    image: Box<str>,
    last_digest: Option<Sha256>,
}

#[derive(Debug, Copy, Clone)]
pub enum ImageWatcherStatus {
    NotUpdated,
    Updated,
    Deleted,
}

impl ImageWatcher {
    pub async fn initialize(image: &str) -> Self {
        log::info!("Initializing watch for {image}");
        let last_digest = match get_latest_digest(image).await {
            Ok(digest) => Some(digest),
            Err(e) => {
                log::warn!("{e}");
                None
            }
        };

        let image = image.into();
        ImageWatcher { image, last_digest }
    }
    pub async fn update(&mut self) -> ImageWatcherStatus {
        let last_digest = self.last_digest;
        let new_sha256 = get_latest_digest(&self.image).await;
        match new_sha256 {
            Err(e) => {
                log::warn!("{e}");
                ImageWatcherStatus::Deleted
            }
            Ok(new_sha256) if last_digest == Some(new_sha256) => ImageWatcherStatus::NotUpdated,
            Ok(new_sha256) => {
                self.last_digest = Some(new_sha256);
                log::info!(
                    "Found a new version for {}, update will start soon...",
                    self.image,
                );
                ImageWatcherStatus::Updated
            }
        }
    }
}

async fn get_latest_digest(image: &str) -> Result<Sha256> {
    // First, pull the latest image
    let pull_result = Command::new("docker")
        .args(["pull"])
        .arg(image)
        .output()
        .await?;

    if !pull_result.status.success() {
        return Err(ImageWatcherError::DockerCommandFailed(
            String::from_utf8_lossy(&pull_result.stderr).to_string(),
        ));
    }

    // Then, inspect the image to get its digest
    let inspect_result = Command::new("docker")
        .args(["inspect"])
        .arg(image)
        .output()
        .await?;

    if !inspect_result.status.success() {
        return Err(ImageWatcherError::DockerCommandFailed(
            String::from_utf8_lossy(&inspect_result.stderr).to_string(),
        ));
    }

    let val: Vec<DockerInspectResponse> = serde_json::from_slice(&inspect_result.stdout)?;
    val.first()
        .ok_or_else(|| {
            ImageWatcherError::DockerCommandFailed("Empty inspect response".to_string())
        })?
        .get_digest()
}
