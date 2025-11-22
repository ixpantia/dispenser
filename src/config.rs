use rayon::prelude::*;

use std::{
    num::NonZeroU64,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use cron::Schedule;

use crate::{
    instance::{Instance, Instances},
    manifests::DockerWatcher,
};

pub struct ContposeConfig {
    pub delay: NonZeroU64,
    pub instances: Vec<ContposeInstanceConfig>,
}

impl ContposeConfig {
    pub fn get_instances(&self) -> Instances {
        let inner = self
            .instances
            .par_iter()
            .with_max_len(1)
            .cloned()
            .map(|instance| Arc::new(Mutex::new(Instance::new(instance))))
            .collect::<Vec<_>>();

        let delay = std::time::Duration::from_secs(self.delay.get());
        Instances { inner, delay }
    }
}

/// Defines when a service should be initialized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Initialize {
    /// The service is started as soon as the application starts.
    Immediately,
    /// The service is started only when a trigger occurs (e.g., a cron schedule or a detected image update).
    OnTrigger,
}

#[derive(Clone)]
pub struct ContposeInstanceConfig {
    pub path: PathBuf,
    pub images: Vec<Image>,
    pub cron: Option<Schedule>,
    /// Defines when the service should be initialized.
    ///
    /// - `Immediately` (default): The service is started as soon as the application starts.
    /// - `OnTrigger`: The service is started only when a trigger occurs (e.g., a cron schedule or a detected image update).
    pub initialize: Initialize,
}

#[derive(Clone)]
pub(crate) struct Image {
    pub(crate) registry: String,
    pub(crate) name: String,
    pub(crate) tag: String,
}

impl ContposeInstanceConfig {
    pub fn get_watchers(&self) -> Vec<DockerWatcher> {
        self.images
            .iter()
            .map(|image| DockerWatcher::initialize(&image.registry, &image.name, &image.tag))
            .collect()
    }
}
