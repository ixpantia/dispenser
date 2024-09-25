use crate::login::{registry, token, user};

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

pub struct DockerWatcher {
    image: Box<str>,
    tag: Box<str>,
    last_digest: Sha256,
}

#[derive(Debug, Copy, Clone)]
pub enum DockerWatcherStatus {
    NotUpdated,
    Updated,
    Deleted,
}

impl DockerWatcher {
    pub fn initialize(image: &str, tag: &str) -> Self {
        let last_digest = get_latest_digest(registry(), user(), token(), image, tag)
            .expect("There is no initial image digest");
        let image = image.into();
        let tag = tag.into();
        DockerWatcher {
            image,
            last_digest,
            tag,
        }
    }
    pub fn update(&mut self) -> DockerWatcherStatus {
        let new_sha256 = get_latest_digest(registry(), user(), token(), &self.image, &self.tag);
        match new_sha256 {
            None => DockerWatcherStatus::Deleted,
            Some(new_sha256) if self.last_digest == new_sha256 => DockerWatcherStatus::NotUpdated,
            Some(new_sha256) => {
                self.last_digest = new_sha256;
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
