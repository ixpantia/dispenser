use chrono::{DateTime, Local};
use cron::Schedule;

use crate::config::ContposeInstanceConfig;
use crate::manifests::{DockerWatcher, DockerWatcherStatus};
use crate::master::{Action, DockerComposeMaster, MasterMsg};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Instances {
    pub inner: Vec<Arc<Mutex<Instance>>>,
    pub delay: std::time::Duration,
}

struct CronWatcher {
    schedule: Schedule,
    next: Option<DateTime<Local>>,
}

impl CronWatcher {
    fn new(schedule: &Schedule) -> Self {
        let schedule = schedule.clone();
        let next = schedule.upcoming(Local).next();
        Self { schedule, next }
    }
    fn is_ready(&mut self) -> bool {
        match self.next {
            Some(next) if chrono::Local::now() >= next => {
                self.next = self.schedule.upcoming(Local).next();
                return true;
            }
            Some(_) | None => false,
        }
    }
}

pub struct Instance {
    pub master: Arc<DockerComposeMaster>,
    watchers: Vec<DockerWatcher>,
    pub config: ContposeInstanceConfig,
    cron_watcher: Option<CronWatcher>,
}

impl Instance {
    pub fn new(config: ContposeInstanceConfig) -> Self {
        // Create a docker-compose master.
        // This represents a process that manages
        // when docker compose is lifted or destroyed
        let cron_watcher = config.cron.as_ref().map(CronWatcher::new);
        let master = Arc::new(DockerComposeMaster::initialize(
            &config.path,
            config.initialize,
            config.vars.clone(),
        ));
        let watchers = config.get_watchers();
        Self {
            master,
            config,
            watchers,
            cron_watcher,
        }
    }
    pub fn poll(&mut self, poll_images: bool) {
        // If uses cron
        if let Some(cron_watcher) = &mut self.cron_watcher {
            if cron_watcher.is_ready() {
                self.master.send_msg(MasterMsg::Update(Action::Recreate));
                log::info!(
                    "Triggering {:?}! Next scheduled trigger at {:?}",
                    self.config.path,
                    cron_watcher.next
                );
                // If the cron matches we can short cirtcuit the function
                return;
            }
        }

        // If its ready to poll images
        if poll_images {
            // try to update the watchers and check
            // if any of them were updated
            let any_updated = self
                .watchers
                .iter()
                .any(|img| matches!(img.update(), DockerWatcherStatus::Updated));

            // If any of the watchers were updated then we
            // send a message to the master to update
            if any_updated {
                self.master.send_msg(MasterMsg::Update(Action::Update));
            }
        }
    }
}
