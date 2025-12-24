use std::{collections::HashMap, path::PathBuf, time::Duration};

use chrono::{DateTime, Local};
use cron::Schedule;

use crate::service::{
    file::{
        DependsOnCondition, DispenserConfig, Initialize, Network, PortEntry, Restart, ServiceEntry,
        VolumeEntry,
    },
    manifest::{ImageWatcher, ImageWatcherStatus},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronWatcher {
    schedule: Schedule,
    next: Option<DateTime<Local>>,
}

impl CronWatcher {
    pub fn new(schedule: &Schedule) -> Self {
        let schedule = schedule.clone();
        let next = schedule.upcoming(Local).next();
        Self { schedule, next }
    }
    fn is_ready(&mut self) -> bool {
        match self.next {
            Some(next) if chrono::Local::now() >= next => {
                self.next = self.schedule.upcoming(Local).next();
                true
            }
            Some(_) | None => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceInstance {
    pub dir: PathBuf,
    pub service: ServiceEntry,
    pub ports: Vec<PortEntry>,
    pub volume: Vec<VolumeEntry>,
    pub env: HashMap<String, String>,
    pub restart: Restart,
    pub network: Vec<Network>,
    pub dispenser: DispenserConfig,
    pub depends_on: HashMap<String, DependsOnCondition>,
    pub cron_watcher: Option<CronWatcher>,
    pub image_watcher: Option<ImageWatcher>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerStatus {
    Running,
    Exited(i32),
    NotFound,
}

/// Parse memory string (e.g., "512m", "2g") to bytes
fn parse_memory_to_bytes(memory_str: &str) -> i64 {
    let memory_str = memory_str.trim().to_lowercase();
    let (value, unit) = if memory_str.ends_with("k") || memory_str.ends_with("kb") {
        let val = memory_str.trim_end_matches("kb").trim_end_matches("k");
        (val, 1024i64)
    } else if memory_str.ends_with("m") || memory_str.ends_with("mb") {
        let val = memory_str.trim_end_matches("mb").trim_end_matches("m");
        (val, 1024i64 * 1024)
    } else if memory_str.ends_with("g") || memory_str.ends_with("gb") {
        let val = memory_str.trim_end_matches("gb").trim_end_matches("g");
        (val, 1024i64 * 1024 * 1024)
    } else if memory_str.ends_with("b") {
        let val = memory_str.trim_end_matches("b");
        (val, 1i64)
    } else {
        // Assume bytes if no unit
        (memory_str.as_str(), 1i64)
    };

    value.parse::<i64>().unwrap_or(0) * unit
}

/// Parse CPU string (e.g., "1.5", "2") to nano CPUs (1 CPU = 1e9 nano CPUs)
fn parse_cpus_to_nano(cpus_str: &str) -> i64 {
    let cpus: f64 = cpus_str.trim().parse().unwrap_or(0.0);
    (cpus * 1_000_000_000.0) as i64
}

/// This function queries the status of a container
/// Returns whether it's up, exited successfully (0 exit status), or failed
async fn get_container_status(container_name: &str) -> Result<ContainerStatus, std::io::Error> {
    let output = tokio::process::Command::new("docker")
        .args([
            "inspect",
            "--format",
            "{{.State.Status}},{{.State.ExitCode}}",
            container_name,
        ])
        .output()
        .await?;

    if !output.status.success() {
        return Ok(ContainerStatus::NotFound);
    }

    let status_str = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = status_str.trim().split(',').collect();

    match parts.as_slice() {
        [status, _exit_code] if *status == "running" => Ok(ContainerStatus::Running),
        [_, exit_code] => {
            let code = exit_code.parse::<i32>().unwrap_or(-1);
            Ok(ContainerStatus::Exited(code))
        }
        _ => Ok(ContainerStatus::NotFound),
    }
}

impl ServiceInstance {
    pub async fn run_container(&self) -> Result<(), std::io::Error> {
        let mut depends_on_conditions = Vec::with_capacity(self.depends_on.len());
        loop {
            for (container, condition) in &self.depends_on {
                let status = match get_container_status(container).await {
                    Ok(status) => match condition {
                        DependsOnCondition::ServiceStarted => {
                            matches!(status, ContainerStatus::Running)
                        }
                        DependsOnCondition::ServiceCompleted => {
                            matches!(status, ContainerStatus::Exited(0))
                        }
                    },
                    Err(_) => false,
                };
                if !status {
                    log::info!(
                        "Service {} is waiting for {} ({:?})",
                        self.service.name,
                        container,
                        condition
                    );
                }
                depends_on_conditions.push(status)
            }
            if depends_on_conditions.iter().all(|&c| c) {
                break;
            }
            depends_on_conditions.clear();
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        // If we are trying to run a container that does exists,
        // create it!
        if self.container_does_not_exist().await {
            self.recreate_container().await?;
        }

        let output = tokio::process::Command::new("docker")
            .args(["start", &self.service.name])
            .output()
            .await?;

        if output.status.success() {
            log::info!("Container {} started successfully", self.service.name);
            Ok(())
        } else {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            log::error!(
                "Failed to start container {}: {}",
                self.service.name,
                error_msg
            );
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to start container: {}", error_msg),
            ))
        }
    }
    pub async fn pull_image(&self) -> Result<(), std::io::Error> {
        log::info!("Pulling image: {}", self.service.image);
        let output = tokio::process::Command::new("docker")
            .args(["pull", &self.service.image])
            .output()
            .await?;

        if output.status.success() {
            log::info!("Image {} pulled successfully", self.service.image);
            Ok(())
        } else {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            log::error!("Failed to pull image {}: {}", self.service.image, error_msg);
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to pull image: {}", error_msg),
            ))
        }
    }

    pub async fn stop_container(&self) -> Result<(), std::io::Error> {
        log::info!("Stopping container: {}", self.service.name);
        let output = tokio::process::Command::new("docker")
            .args(["stop", &self.service.name])
            .output()
            .await?;

        if output.status.success() {
            log::info!("Container {} stopped successfully", self.service.name);
            Ok(())
        } else {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            log::warn!(
                "Failed to stop container {}: {}",
                self.service.name,
                error_msg
            );
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to warn container: {}", error_msg),
            ))
        }
    }

    pub async fn remove_container(&self) -> Result<(), std::io::Error> {
        log::info!("Removing container: {}", self.service.name);
        let output = tokio::process::Command::new("docker")
            .args(["rm", "-f", &self.service.name])
            .output()
            .await?;

        if output.status.success() {
            log::info!("Container {} removed successfully", self.service.name);
            Ok(())
        } else {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            log::error!(
                "Failed to remove container {}: {}",
                self.service.name,
                error_msg
            );
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to remove container: {}", error_msg),
            ))
        }
    }

    pub async fn create_container(&self) -> Result<(), std::io::Error> {
        log::info!("Creating container: {}", self.service.name);

        let mut cmd = tokio::process::Command::new("docker");
        cmd.arg("create");
        cmd.args(["--name", &self.service.name]);

        // Add restart policy
        match self.restart {
            Restart::Always => cmd.args(["--restart", "always"]),
            Restart::No => cmd.args(["--restart", "no"]),
            Restart::OnFailure => cmd.args(["--restart", "on-failure"]),
            Restart::UnlessStopped => cmd.args(["--restart", "unless-stopped"]),
        };

        // Add port mappings
        for port in &self.ports {
            cmd.args(["-p", &format!("{}:{}", port.host, port.container)]);
        }

        // Add volume mappings
        for volume in &self.volume {
            let mount_str = if volume.readonly {
                format!("{}:{}:ro", volume.source, volume.target)
            } else {
                format!("{}:{}", volume.source, volume.target)
            };
            cmd.args(["-v", &mount_str]);
        }

        // Add environment variables
        for (key, value) in &self.env {
            cmd.args(["-e", &format!("{}={}", key, value)]);
        }

        // Add networks
        for network in &self.network {
            cmd.args(["--network", &network.name]);
        }

        // Add resource limits
        if let Some(memory) = &self.service.memory {
            cmd.args(["--memory", memory]);
        }
        if let Some(cpus) = &self.service.cpus {
            cmd.args(["--cpus", cpus]);
        }

        // Add working directory
        if let Some(working_dir) = &self.service.working_dir {
            cmd.args(["--workdir", working_dir]);
        }

        // Add user
        if let Some(user) = &self.service.user {
            cmd.args(["--user", user]);
        }

        // Add hostname
        if let Some(hostname) = &self.service.hostname {
            cmd.args(["--hostname", hostname]);
        }

        // Add entrypoint if specified
        if let Some(entrypoint) = &self.service.entrypoint {
            cmd.arg("--entrypoint");
            cmd.arg(entrypoint.join(" "));
        }

        // Add the image
        cmd.arg(&self.service.image);

        if let Some(command) = &self.service.command {
            cmd.args(command);
        }

        // Set the directory for the command
        cmd.current_dir(&self.dir);

        let output = cmd.output().await?;

        if output.status.success() {
            log::info!("Container {} created successfully", self.service.name);
            Ok(())
        } else {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            log::error!(
                "Failed to create container {}: {}",
                self.service.name,
                error_msg
            );
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to create container: {}", error_msg),
            ))
        }
    }

    pub async fn recreate_container(&self) -> Result<(), std::io::Error> {
        self.pull_image().await?;
        let _ = self.stop_container().await;
        let _ = self.remove_container().await;
        self.create_container().await?;
        Ok(())
    }

    pub async fn container_does_not_exist(&self) -> bool {
        // Get the container inspection data
        let output = match tokio::process::Command::new("docker")
            .args(["inspect", "--format", "{{json .}}", &self.service.name])
            .output()
            .await
        {
            Ok(output) => output,
            Err(e) => {
                log::warn!("Failed to inspect container {}: {}", self.service.name, e);
                return true; // If we can't inspect, assume recreate is needed
            }
        };

        if !output.status.success() {
            log::info!(
                "Container {} does not exist, needs creation",
                self.service.name
            );
            return true;
        }
        false
    }

    /// Validate if the current container is different from
    /// this instance or if it does not exist.
    pub async fn requires_recreate(&self, other: &Self) -> bool {
        if self.container_does_not_exist().await {
            return true;
        }
        // If self and other are not equal we need to recreate the
        // container
        self != other
    }

    pub async fn recreate_if_required(&self, other: &Self) {
        if self.requires_recreate(other).await {
            if let Err(e) = self.recreate_container().await {
                log::error!("Failed to recreate container {}: {}", self.service.name, e);
            }
        }
    }

    pub async fn poll(&mut self, poll_images: bool, init: bool) {
        if init && self.dispenser.initialize == Initialize::Immediately {
            log::info!("Starting {} immediately", self.service.name);
            if let Err(e) = self.run_container().await {
                log::error!("Failed to run container {}: {}", self.service.name, e);
            }
            return;
        }

        // If uses cron
        if let Some(cron_watcher) = &mut self.cron_watcher {
            if cron_watcher.is_ready() {
                // If the cron matches we can short circuit the function
                if let Err(e) = self.run_container().await {
                    log::error!(
                        "Failed to run container {} from cron: {}",
                        self.service.name,
                        e
                    );
                }

                return;
            }
        }

        // If its ready to poll images
        if self.dispenser.watch && poll_images {
            // try to update the watchers and check
            // if any of them were updated
            if let Some(image_watcher) = &mut self.image_watcher {
                match image_watcher.update().await {
                    ImageWatcherStatus::Updated => {
                        log::info!(
                            "Image updated for service {}, recreating container...",
                            self.service.name
                        );
                        if let Err(e) = self.recreate_container().await {
                            log::error!(
                                "Failed to recreate container {}: {}",
                                self.service.name,
                                e
                            );
                        }
                        if let Err(e) = self.run_container().await {
                            log::error!("Failed to run container {}: {}", self.service.name, e);
                        }
                    }
                    ImageWatcherStatus::Deleted => {
                        log::warn!("Image for service {} was deleted", self.service.name);
                    }
                    ImageWatcherStatus::NotUpdated => {}
                }
            }
        }
    }
}
