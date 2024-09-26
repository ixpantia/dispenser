use crate::login::get_docker_credentials;
use std::sync::{Arc, Mutex};

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
    pub fn get_digest(&self, architecture: &str, os: &str) -> Option<Sha256> {
        if let Some(config) = self.config.as_ref() {
            let mut inner = [0u8; 64];
            inner.copy_from_slice(
                config
                    .digest
                    .strip_prefix("sha256:")
                    .expect("Digest is not sha256")
                    .as_bytes(),
            );
            return Some(Sha256 { inner });
        }
        if let Some(manifests) = self.manifests.as_ref() {
            for man in manifests {
                if man.platform.architecture == architecture && man.platform.os == os {
                    let mut inner = [0u8; 64];
                    inner.copy_from_slice(
                        man.digest
                            .strip_prefix("sha256:")
                            .expect("Digest is not sha256")
                            .as_bytes(),
                    );
                    return Some(Sha256 { inner });
                }
            }
        }
        None
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
    last_digest: Arc<Mutex<Sha256>>,
}

#[derive(Debug, Copy, Clone)]
pub enum DockerWatcherStatus {
    NotUpdated,
    Updated,
    Deleted,
}

impl DockerWatcher {
    pub fn initialize(registry: &str, image: &str, tag: &str) -> Self {
        log::info!("Initializing watch for {registry}/{image}:{tag}");
        let (user, password) = get_docker_credentials(registry);
        let last_digest = Arc::new(Mutex::new(
            get_latest_digest(registry, &user, &password, image, tag)
                .expect("There is no initial image digest"),
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
    pub fn update(&self) -> DockerWatcherStatus {
        let last_digest = *self.last_digest.lock().expect("Unable to lock mutex");
        let (user, password) = get_docker_credentials(&self.registry);
        let new_sha256 =
            get_latest_digest(&self.registry, &user, &password, &self.image, &self.tag);
        match new_sha256 {
            None => DockerWatcherStatus::Deleted,
            Some(new_sha256) if last_digest == new_sha256 => DockerWatcherStatus::NotUpdated,
            Some(new_sha256) => {
                let mut last_digest = self.last_digest.lock().expect("Unable to lock mutex");
                *last_digest = new_sha256;
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

fn get_latest_digest(
    registry: &str,
    user: &str,
    token: &str,
    image: &str,
    tag: &str,
) -> Option<Sha256> {
    let url = format!("https://{user}:{token}@{registry}/v2/{image}/manifests/{tag}");
    let val: DockerManifestsResponse = ureq::get(&url).call().unwrap().into_json().unwrap();
    val.get_digest("amd64", "linux")
}
