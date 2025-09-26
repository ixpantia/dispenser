use rayon::prelude::*;

use std::{
    num::NonZeroU64,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use cron::Schedule;

use crate::{
    instance::{Instance, Instances},
    manifests::DockerWatcher,
};

#[derive(serde::Deserialize)]
pub struct ContposeConfig {
    pub delay: NonZeroU64,
    #[serde(default)]
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
            .par_iter()
            .with_max_len(1)
            .cloned()
            .map(|instance| Arc::new(Mutex::new(Instance::new(instance))))
            .collect::<Vec<_>>();

        let delay = std::time::Duration::from_secs(self.delay.get());
        Instances { inner, delay }
    }
}

#[derive(serde::Deserialize, Clone)]
pub struct ContposeInstanceConfig {
    pub path: PathBuf,
    #[serde(default)]
    images: Vec<Image>,
    #[serde(default)]
    pub cron: Option<Schedule>,
}

#[derive(serde::Deserialize, Clone)]
struct Image {
    registry: String,
    name: String,
    tag: String,
}

impl ContposeInstanceConfig {
    pub fn get_watchers(&self) -> Vec<DockerWatcher> {
        self.images
            .iter()
            .map(|image| DockerWatcher::initialize(&image.registry, &image.name, &image.tag))
            .collect()
    }
}
