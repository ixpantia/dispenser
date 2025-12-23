use std::{collections::HashMap, path::PathBuf};

use chrono::{DateTime, Local};
use cron::Schedule;

use crate::service::{
    file::{
        DependsOnCondition, DispenserConfig, Initialize, Network, PortEntry, Restart, ServiceEntry,
        VolumeEntry,
    },
    manifest::{ImageWatcher, ImageWatcherStatus},
};

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
                depends_on_conditions.push(status)
            }
            if depends_on_conditions.iter().all(|&c| c) {
                break;
            }
            depends_on_conditions.clear();
        }

        if let Err(e) = self.pull_image().await {
            log::error!("Failed to pull image for {}: {}", self.service.name, e);
        }
        self.recreate_if_required().await;

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
            log::error!(
                "Failed to stop container {}: {}",
                self.service.name,
                error_msg
            );
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to stop container: {}", error_msg),
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
            cmd.args(["-v", &format!("{}:{}", volume.source, volume.target)]);
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

        // Add the image
        cmd.arg(&self.service.image);

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

    /// Validate if the current container is different from
    /// this instance or if it does not exist.
    ///
    /// If anything has changed like: environment variables, volumes, ports, etc we need to recreate
    pub async fn requires_recreate(&self) -> bool {
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

        let inspect_str = String::from_utf8_lossy(&output.stdout);
        let inspect_json: serde_json::Value = match serde_json::from_str(&inspect_str) {
            Ok(json) => json,
            Err(e) => {
                log::warn!("Failed to parse docker inspect JSON: {}", e);
                return true;
            }
        };

        // Check if the image has changed
        let current_image = inspect_json["Config"]["Image"].as_str().unwrap_or("");
        if current_image != self.service.image {
            log::info!(
                "Image changed for {}: {} -> {}",
                self.service.name,
                current_image,
                self.service.image
            );
            return true;
        }

        // Check restart policy
        let current_restart = inspect_json["HostConfig"]["RestartPolicy"]["Name"]
            .as_str()
            .unwrap_or("");
        let expected_restart = match self.restart {
            Restart::Always => "always",
            Restart::No => "no",
            Restart::OnFailure => "on-failure",
            Restart::UnlessStopped => "unless-stopped",
        };
        if current_restart != expected_restart {
            log::info!(
                "Restart policy changed for {}: {} -> {}",
                self.service.name,
                current_restart,
                expected_restart
            );
            return true;
        }

        // Check environment variables
        if let Some(current_env) = inspect_json["Config"]["Env"].as_array() {
            let mut current_env_map = HashMap::new();
            for env_str in current_env {
                if let Some(s) = env_str.as_str() {
                    if let Some(pos) = s.find('=') {
                        let (key, value) = s.split_at(pos);
                        current_env_map.insert(key.to_string(), value[1..].to_string());
                    }
                }
            }

            for (key, value) in &self.env {
                if current_env_map.get(key) != Some(value) {
                    log::info!(
                        "Environment variable changed for {}: {}",
                        self.service.name,
                        key
                    );
                    return true;
                }
            }
        }

        // Check port bindings
        if let Some(port_bindings) = inspect_json["HostConfig"]["PortBindings"].as_object() {
            for port in &self.ports {
                let container_port_key = format!("{}/tcp", port.container);
                if let Some(bindings) = port_bindings.get(&container_port_key) {
                    if let Some(binding_array) = bindings.as_array() {
                        if binding_array.is_empty() {
                            log::info!("Port binding changed for {}", self.service.name);
                            return true;
                        }
                        let host_port = binding_array[0]["HostPort"].as_str().unwrap_or("");
                        if host_port != port.host.to_string() {
                            log::info!(
                                "Port mapping changed for {}: {} -> {}",
                                self.service.name,
                                host_port,
                                port.host
                            );
                            return true;
                        }
                    }
                } else {
                    log::info!("Port binding missing for {}", self.service.name);
                    return true;
                }
            }
        } else if !self.ports.is_empty() {
            log::info!("Port bindings changed for {}", self.service.name);
            return true;
        }

        // Check volume bindings
        if let Some(binds) = inspect_json["HostConfig"]["Binds"].as_array() {
            let current_binds: Vec<String> = binds
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();

            for volume in &self.volume {
                // Normalize the source path to an absolute path for comparison
                let source_path = if std::path::Path::new(&volume.source).is_relative() {
                    self.dir
                        .join(&volume.source)
                        .canonicalize()
                        .unwrap_or_else(|_| self.dir.join(&volume.source))
                        .to_string_lossy()
                        .to_string()
                } else {
                    volume.source.clone()
                };

                let expected_bind = format!("{}:{}", source_path, volume.target);
                if !current_binds.iter().any(|b| b == &expected_bind) {
                    log::info!(
                        "Volume binding changed for {}: {}",
                        self.service.name,
                        expected_bind
                    );
                    return true;
                }
            }
        } else if !self.volume.is_empty() {
            log::info!("Volume bindings changed for {}", self.service.name);
            return true;
        }

        // Check networks
        if let Some(networks) = inspect_json["NetworkSettings"]["Networks"].as_object() {
            for network in &self.network {
                if !networks.contains_key(&network.name) {
                    log::info!(
                        "Network changed for {}: {}",
                        self.service.name,
                        network.name
                    );
                    return true;
                }
            }
        } else if !self.network.is_empty() {
            log::info!("Networks changed for {}", self.service.name);
            return true;
        }

        // Check memory limit
        if let Some(expected_memory) = &self.service.memory {
            let current_memory = inspect_json["HostConfig"]["Memory"].as_i64().unwrap_or(0);
            // Parse expected memory string (e.g., "512m", "2g") to bytes
            let expected_bytes = parse_memory_to_bytes(expected_memory);
            if current_memory != expected_bytes {
                log::info!(
                    "Memory limit changed for {}: {} -> {}",
                    self.service.name,
                    current_memory,
                    expected_bytes
                );
                return true;
            }
        } else {
            // Check if container has a memory limit but we don't expect one
            let current_memory = inspect_json["HostConfig"]["Memory"].as_i64().unwrap_or(0);
            if current_memory != 0 {
                log::info!("Memory limit changed for {} (removed)", self.service.name);
                return true;
            }
        }

        // Check CPU limit
        if let Some(expected_cpus) = &self.service.cpus {
            let current_cpus = inspect_json["HostConfig"]["NanoCpus"].as_i64().unwrap_or(0);
            // Parse expected CPUs string to nano CPUs (1 CPU = 1e9 nano CPUs)
            let expected_nano_cpus = parse_cpus_to_nano(expected_cpus);
            if current_cpus != expected_nano_cpus {
                log::info!(
                    "CPU limit changed for {}: {} -> {}",
                    self.service.name,
                    current_cpus,
                    expected_nano_cpus
                );
                return true;
            }
        } else {
            // Check if container has a CPU limit but we don't expect one
            let current_cpus = inspect_json["HostConfig"]["NanoCpus"].as_i64().unwrap_or(0);
            if current_cpus != 0 {
                log::info!("CPU limit changed for {} (removed)", self.service.name);
                return true;
            }
        }

        false
    }

    pub async fn recreate_if_required(&self) {
        if self.requires_recreate().await {
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
            if let Some(ref image_watcher) = self.image_watcher {
                match image_watcher.update().await {
                    ImageWatcherStatus::Updated => {
                        log::info!(
                            "Image updated for service {}, recreating container...",
                            self.service.name
                        );
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
