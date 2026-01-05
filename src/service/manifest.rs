use bollard::query_parameters::{CreateImageOptions, CreateImageOptionsBuilder};
use futures_util::StreamExt;
use thiserror::Error;

use crate::service::docker::get_docker;

pub type Result<T> = std::result::Result<T, ImageWatcherError>;

#[derive(Error, Debug)]
pub enum ImageWatcherError {
    #[error("Digest string '{0}' does not start with 'sha256:'")]
    InvalidDigestPrefix(String),
    #[error("JSON deserialization error: {0}")]
    SerdeJsonError(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Docker API error: {0}")]
    DockerApiError(#[from] bollard::errors::Error),
    #[error("Docker command failed: {0}")]
    DockerCommandFailed(String),
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

use tokio::sync::Mutex;

/// ImageWatcher monitors a Docker image for updates by tracking its digest.
///
/// # Equality
///
/// Note: PartialEq and Eq are implemented to compare only the `image` field,
/// ignoring `last_digest`. This allows ImageWatcher instances to be considered
/// equal if they watch the same image, regardless of their current digest state.
#[derive(Debug)]
pub struct ImageWatcher {
    image: Box<str>,
    last_digest: Mutex<Option<Sha256>>,
}

impl PartialEq for ImageWatcher {
    fn eq(&self, other: &Self) -> bool {
        self.image == other.image
    }
}

impl Eq for ImageWatcher {}

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
        ImageWatcher {
            image,
            last_digest: Mutex::new(last_digest),
        }
    }
    pub async fn update(&self) -> ImageWatcherStatus {
        let last_digest = *self.last_digest.lock().await;
        let new_sha256 = get_latest_digest(&self.image).await;
        match new_sha256 {
            Err(e) => {
                log::warn!("{e}");
                ImageWatcherStatus::Deleted
            }
            Ok(new_sha256) if last_digest == Some(new_sha256) => ImageWatcherStatus::NotUpdated,
            Ok(new_sha256) => {
                *self.last_digest.lock().await = Some(new_sha256);
                log::info!(
                    "Found a new version for {}, update will start soon...",
                    self.image,
                );
                ImageWatcherStatus::Updated
            }
        }
    }
}

/// Parse an image reference into (image, tag) components
fn parse_image_reference(image: &str) -> (&str, &str) {
    // Handle digest references (image@sha256:...)
    if let Some(at_pos) = image.find('@') {
        return (&image[..at_pos], &image[at_pos..]);
    }

    // Handle tag references (image:tag)
    // Need to be careful with registry URLs that contain port numbers
    // e.g., localhost:5000/myimage:tag
    if let Some(colon_pos) = image.rfind(':') {
        // Check if the colon is part of a port number in the registry URL
        let after_colon = &image[colon_pos + 1..];
        // If there's a slash after the colon, it's a port number, not a tag
        if !after_colon.contains('/') {
            return (&image[..colon_pos], after_colon);
        }
    }

    // No tag specified, use "latest"
    (image, "latest")
}

async fn get_latest_digest(image: &str) -> Result<Sha256> {
    let docker = get_docker();

    // Parse image name and tag
    let (image_name, tag) = parse_image_reference(image);

    // Pull the latest image using bollard
    let options: CreateImageOptions = CreateImageOptionsBuilder::new()
        .from_image(image_name)
        .tag(tag)
        .build();

    let mut stream = docker.create_image(Some(options), None, None);

    while let Some(result) = stream.next().await {
        match result {
            Ok(info) => {
                if let Some(status) = info.status {
                    log::debug!("Pull status: {}", status);
                }
            }
            Err(e) => {
                return Err(ImageWatcherError::DockerApiError(e));
            }
        }
    }

    // Inspect the image to get its digest
    let inspect = docker.inspect_image(image).await?;

    // Try to get digest from RepoDigests first
    if let Some(repo_digests) = inspect.repo_digests {
        if let Some(first_digest) = repo_digests.first() {
            // RepoDigests format is like "repository@sha256:..."
            if let Some(digest_part) = first_digest.split('@').nth(1) {
                let hash = digest_part.strip_prefix("sha256:").ok_or_else(|| {
                    ImageWatcherError::InvalidDigestPrefix(digest_part.to_string())
                })?;
                let mut inner = [0u8; 64];
                let hash_bytes = hash.as_bytes();
                if hash_bytes.len() >= 64 {
                    inner.copy_from_slice(&hash_bytes[..64]);
                    return Ok(Sha256 { inner });
                }
            }
        }
    }

    // Fallback to Id if RepoDigests is not available
    if let Some(id) = inspect.id {
        let hash = id
            .strip_prefix("sha256:")
            .ok_or_else(|| ImageWatcherError::InvalidDigestPrefix(id.clone()))?;
        let mut inner = [0u8; 64];
        let hash_bytes = hash.as_bytes();
        if hash_bytes.len() >= 64 {
            inner.copy_from_slice(&hash_bytes[..64]);
            return Ok(Sha256 { inner });
        }
    }

    Err(ImageWatcherError::DockerCommandFailed(
        "No digest found in inspect output".to_string(),
    ))
}
