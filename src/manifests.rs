use std::sync::Arc;
use tokio::{process::Command, sync::Mutex};

use thiserror::Error;

pub type Result<T> = std::result::Result<T, DockerWatcherError>;

#[derive(Error, Debug)]
pub enum DockerWatcherError {
    #[error("Digest string '{0}' does not start with 'sha256:'")]
    InvalidDigestPrefix(String),
    #[error("JSON deserialization error: {0}")]
    SerdeJsonError(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("No digest found for architecture '{architecture}' and OS '{os}'")]
    NoMatchingManifest {
        architecture: Box<str>,
        os: Box<str>,
    },
}

#[derive(serde::Deserialize)]
pub struct DockerManifestsResponse {
    config: Option<Config>,
    manifests: Option<Vec<Manifest>>,
}

#[derive(serde::Deserialize)]
struct Config {
    digest: String,
}

impl DockerManifestsResponse {
    pub fn get_digest(&self, architecture: &str, os: &str) -> Result<Sha256> {
        if let Some(config) = self.config.as_ref() {
            let mut inner = [0u8; 64];
            inner.copy_from_slice(
                config
                    .digest
                    .strip_prefix("sha256:")
                    .ok_or_else(|| DockerWatcherError::InvalidDigestPrefix(config.digest.clone()))?
                    .as_bytes(),
            );
            return Ok(Sha256 { inner });
        }
        if let Some(manifests) = self.manifests.as_ref() {
            for man in manifests {
                if man.platform.architecture == architecture && man.platform.os == os {
                    let mut inner = [0u8; 64];
                    inner.copy_from_slice(
                        man.digest
                            .strip_prefix("sha256:")
                            .ok_or_else(|| {
                                DockerWatcherError::InvalidDigestPrefix(man.digest.clone())
                            })?
                            .as_bytes(),
                    );
                    return Ok(Sha256 { inner });
                }
            }
        }
        Err(DockerWatcherError::NoMatchingManifest {
            architecture: architecture.into(),
            os: os.into(),
        })
    }
}

#[derive(serde::Deserialize)]
struct Platform {
    architecture: String,
    os: String,
}

#[derive(serde::Deserialize)]
struct Manifest {
    digest: String,
    platform: Platform,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Sha256 {
    /// 256 bits of data in base64
    pub inner: [u8; 64],
}

#[derive(Clone)]
pub struct DockerWatcher {
    registry: Box<str>,
    image: Box<str>,
    tag: Box<str>,
    last_digest: Arc<Mutex<Option<Sha256>>>,
}

#[derive(Debug, Copy, Clone)]
pub enum DockerWatcherStatus {
    NotUpdated,
    Updated,
    Deleted,
}

impl DockerWatcher {
    pub async fn initialize(registry: &str, image: &str, tag: &str) -> Self {
        log::info!("Initializing watch for {registry}/{image}:{tag}");
        let last_digest = Arc::new(Mutex::new(
            match get_latest_digest(registry, image, tag).await {
                Ok(digest) => Some(digest),
                Err(e) => {
                    log::warn!("{e}");
                    None
                }
            },
        ));

        let registry = registry.into();
        let image = image.into();
        let tag = tag.into();
        DockerWatcher {
            registry,
            image,
            last_digest,
            tag,
        }
    }
    pub async fn update(&self) -> DockerWatcherStatus {
        let last_digest = *self.last_digest.lock().await;
        let new_sha256 = get_latest_digest(&self.registry, &self.image, &self.tag).await;
        match new_sha256 {
            Err(e) => {
                log::warn!("{e}");
                DockerWatcherStatus::Deleted
            }
            Ok(new_sha256) if last_digest == Some(new_sha256) => DockerWatcherStatus::NotUpdated,
            Ok(new_sha256) => {
                let mut last_digest = self.last_digest.lock().await;
                *last_digest = Some(new_sha256);
                log::info!(
                    "Found a new version for {}:{}, update will start soon...",
                    self.image,
                    self.tag
                );
                DockerWatcherStatus::Updated
            }
        }
    }
}

async fn get_latest_digest(registry: &str, image: &str, tag: &str) -> Result<Sha256> {
    let output_result = Command::new("docker")
        .args(["manifest", "inspect"])
        .arg(format!("{registry}/{image}:{tag}"))
        .output()
        .await?;
    let val: DockerManifestsResponse = serde_json::from_slice(&output_result.stdout)?;
    val.get_digest("amd64", "linux")
}
