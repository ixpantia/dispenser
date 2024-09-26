use std::{num::NonZeroU64, path::PathBuf, sync::Arc, time::Duration};

use crate::{
    instance::{Instance, Instances},
    manifests::DockerWatcher,
};

#[derive(serde::Deserialize)]
pub struct ContposeConfig {
    pub delay: NonZeroU64,
    pub instance: Vec<ContposeInstanceConfig>,
}

impl ContposeConfig {
    pub fn init() -> Self {
        Self::try_init().unwrap()
    }
    pub fn try_init() -> Result<Self, Box<dyn std::error::Error>> {
        use std::io::Read;
        let mut config = String::new();
        std::fs::File::open(&crate::cli::get_cli_args().config)?.read_to_string(&mut config)?;
        Ok(toml::from_str(&config)?)
    }
    pub fn get_instances(&self) -> Instances {
        let inner = self
            .instance
            .iter()
            .cloned()
            .map(Instance::new)
            .map(Arc::new)
            .collect();
        let delay = std::time::Duration::from_secs(self.delay.get());
        Instances { inner, delay }
    }
}

#[derive(serde::Deserialize, Clone)]
pub struct ContposeInstanceConfig {
    pub path: PathBuf,
    pub interval: Option<u64>,
    images: Vec<Image>,
}

#[derive(serde::Deserialize, Clone)]
struct Image {
    registry: String,
    name: String,
    tag: String,
}

impl ContposeInstanceConfig {
    pub fn get_interval(&self) -> Duration {
        std::time::Duration::from_secs(self.interval.unwrap_or(5))
    }
    pub fn get_watchers(&self) -> Vec<DockerWatcher> {
        self.images
            .iter()
            .map(|image| DockerWatcher::initialize(&image.registry, &image.name, &image.tag))
            .collect()
    }
}
