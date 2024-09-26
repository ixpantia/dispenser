use crate::config::ContposeInstanceConfig;
use crate::manifests::{DockerWatcher, DockerWatcherStatus};
use crate::master::{DockerComposeMaster, MasterMsg};
use std::sync::Arc;

#[derive(Clone)]
pub struct Instances {
    pub inner: Vec<Arc<Instance>>,
    pub delay: std::time::Duration,
}

#[derive(Clone)]
pub struct Instance {
    pub master: Arc<DockerComposeMaster>,
    watchers: Vec<DockerWatcher>,
    pub config: ContposeInstanceConfig,
}

impl Instance {
    pub fn new(config: ContposeInstanceConfig) -> Self {
        // Create a docker-compose master.
        // This represents a process that manages
        // when docker compose is lifted or destroyed
        let master = Arc::new(DockerComposeMaster::initialize(&config.path));
        let watchers = config.get_watchers();
        Self {
            master,
            config,
            watchers,
        }
    }
    pub fn poll(&self) {
        // try to update the watchers and check
        // if any of them were updated
        let any_updated = self
            .watchers
            .iter()
            .any(|img| matches!(img.update(), DockerWatcherStatus::Updated));

        // If any of the watchers were updated then we
        // send a message to the master to update
        if any_updated {
            self.master.send_msg(MasterMsg::Update);
        }
    }
}
