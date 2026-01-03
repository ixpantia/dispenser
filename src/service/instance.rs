use std::{collections::HashMap, path::PathBuf, time::Duration};

use bollard::models::{
    ContainerCreateBody, EndpointSettings, HostConfig, NetworkConnectRequest, PortBinding,
    RestartPolicy, RestartPolicyNameEnum,
};
use bollard::query_parameters::{
    CreateContainerOptions, CreateContainerOptionsBuilder, CreateImageOptions,
    CreateImageOptionsBuilder, InspectContainerOptions, InspectContainerOptionsBuilder,
    RemoveContainerOptions, RemoveContainerOptionsBuilder, StartContainerOptions,
    StartContainerOptionsBuilder, StopContainerOptions, StopContainerOptionsBuilder,
};
use chrono::{DateTime, Local};
use cron::Schedule;
use futures_util::StreamExt;

use crate::service::vars::ServiceConfigError;
use crate::service::{
    docker::get_docker,
    file::{
        DependsOnCondition, DispenserConfig, Initialize, Network, PortEntry, PullOptions, Restart,
        ServiceEntry, VolumeEntry,
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

/// This function queries the status of a container using bollard
/// Returns whether it's up, exited successfully (0 exit status), or failed
async fn get_container_status(container_name: &str) -> Result<ContainerStatus, ServiceConfigError> {
    let docker = get_docker();

    let options: InspectContainerOptions = InspectContainerOptionsBuilder::new().build();

    match docker
        .inspect_container(container_name, Some(options))
        .await
    {
        Ok(info) => {
            if let Some(state) = info.state {
                if state.running.unwrap_or(false) {
                    return Ok(ContainerStatus::Running);
                }
                let exit_code = state.exit_code.unwrap_or(-1) as i32;
                return Ok(ContainerStatus::Exited(exit_code));
            }
            Ok(ContainerStatus::NotFound)
        }
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 404, ..
        }) => Ok(ContainerStatus::NotFound),
        Err(e) => Err(ServiceConfigError::DockerApi(e)),
    }
}

impl ServiceInstance {
    pub async fn run_container(&self) -> Result<(), ServiceConfigError> {
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

        if self.dispenser.pull == PullOptions::Always || self.container_does_not_exist().await {
            self.recreate_container().await?;
        }

        let docker = get_docker();

        let options: StartContainerOptions = StartContainerOptionsBuilder::new().build();

        docker
            .start_container(&self.service.name, Some(options))
            .await
            .inspect_err(|e| {
                log::error!("Failed to start container {}: {}", self.service.name, e);
            })?;

        log::info!("Container {} started successfully", self.service.name);
        Ok(())
    }

    pub async fn pull_image(&self) -> Result<(), ServiceConfigError> {
        log::info!("Pulling image: {}", self.service.image);
        let docker = get_docker();

        // Parse image name and tag
        let (image, tag) = parse_image_reference(&self.service.image);

        let options: CreateImageOptions = CreateImageOptionsBuilder::new()
            .from_image(image)
            .tag(tag)
            .build();

        let mut stream = docker.create_image(Some(options), None, None);

        while let Some(result) = stream.next().await {
            match result {
                Ok(info) => {
                    if let Some(status) = info.status {
                        log::debug!("Pull status: {}", status);
                    }
                }
                Err(e) => {
                    log::error!("Failed to pull image {}: {}", self.service.image, e);
                    return Err(ServiceConfigError::DockerApi(e));
                }
            }
        }

        log::info!("Image {} pulled successfully", self.service.image);
        Ok(())
    }

    pub async fn stop_container(&self) -> Result<(), ServiceConfigError> {
        log::info!("Stopping container: {}", self.service.name);
        let docker = get_docker();

        let options: StopContainerOptions = StopContainerOptionsBuilder::new().t(10).build();

        match docker
            .stop_container(&self.service.name, Some(options))
            .await
        {
            Ok(_) => {
                log::info!("Container {} stopped successfully", self.service.name);
                Ok(())
            }
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => {
                log::warn!("Container {} not found, skipping stop", self.service.name);
                Ok(())
            }
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 304, ..
            }) => {
                log::info!("Container {} already stopped", self.service.name);
                Ok(())
            }
            Err(e) => {
                log::warn!("Failed to stop container {}: {}", self.service.name, e);
                Err(ServiceConfigError::DockerApi(e))
            }
        }
    }

    pub async fn remove_container(&self) -> Result<(), ServiceConfigError> {
        log::info!("Removing container: {}", self.service.name);
        let docker = get_docker();

        let options: RemoveContainerOptions =
            RemoveContainerOptionsBuilder::new().force(true).build();

        match docker
            .remove_container(&self.service.name, Some(options))
            .await
        {
            Ok(_) => {
                log::info!("Container {} removed successfully", self.service.name);
                Ok(())
            }
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => {
                log::info!(
                    "Container {} not found, skipping removal",
                    self.service.name
                );
                Ok(())
            }
            Err(e) => {
                log::error!("Failed to remove container {}: {}", self.service.name, e);
                Err(ServiceConfigError::DockerApi(e))
            }
        }
    }

    pub async fn create_container(&self) -> Result<(), ServiceConfigError> {
        log::info!("Creating container: {}", self.service.name);
        let docker = get_docker();

        // Build port bindings
        let mut port_bindings: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();
        let mut exposed_ports: HashMap<String, HashMap<(), ()>> = HashMap::new();

        for port in &self.ports {
            let container_port = format!("{}/tcp", port.container);
            exposed_ports.insert(container_port.clone(), HashMap::new());
            port_bindings.insert(
                container_port,
                Some(vec![PortBinding {
                    host_ip: Some("0.0.0.0".to_string()),
                    host_port: Some(port.host.to_string()),
                }]),
            );
        }

        // Build volume bindings
        let binds: Vec<String> = self
            .volume
            .iter()
            .map(|v| {
                let source = v.normalized_source(&self.dir)?;
                if v.readonly {
                    Ok(format!("{}:{}:ro", source, v.target))
                } else {
                    Ok(format!("{}:{}", source, v.target))
                }
            })
            .collect::<Result<_, ServiceConfigError>>()?;

        // Build environment variables
        let env: Vec<String> = self
            .env
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();

        // Build restart policy
        let restart_policy = match self.restart {
            Restart::Always => Some(RestartPolicy {
                name: Some(RestartPolicyNameEnum::ALWAYS),
                maximum_retry_count: None,
            }),
            Restart::No => Some(RestartPolicy {
                name: Some(RestartPolicyNameEnum::NO),
                maximum_retry_count: None,
            }),
            Restart::OnFailure => Some(RestartPolicy {
                name: Some(RestartPolicyNameEnum::ON_FAILURE),
                maximum_retry_count: None,
            }),
            Restart::UnlessStopped => Some(RestartPolicy {
                name: Some(RestartPolicyNameEnum::UNLESS_STOPPED),
                maximum_retry_count: None,
            }),
        };

        // Parse memory limit
        let memory = self.service.memory.as_ref().map(|m| parse_memory_limit(m));

        // Parse CPU limit (convert to nano CPUs)
        let nano_cpus = self.service.cpus.as_ref().map(|c| {
            let cpus: f64 = c.parse().unwrap_or(1.0);
            (cpus * 1_000_000_000.0) as i64
        });

        // Build host config
        let host_config = HostConfig {
            binds: if binds.is_empty() { None } else { Some(binds) },
            port_bindings: if port_bindings.is_empty() {
                None
            } else {
                Some(port_bindings)
            },
            restart_policy,
            memory,
            nano_cpus,
            network_mode: self.network.first().map(|n| n.name.clone()),
            ..Default::default()
        };

        // Build container config
        let config = ContainerCreateBody {
            image: Some(self.service.image.clone()),
            hostname: self.service.hostname.clone(),
            user: self.service.user.clone(),
            working_dir: self.service.working_dir.clone(),
            env: if env.is_empty() { None } else { Some(env) },
            cmd: self.service.command.clone(),
            entrypoint: self.service.entrypoint.clone(),
            exposed_ports: if exposed_ports.is_empty() {
                None
            } else {
                Some(exposed_ports)
            },
            host_config: Some(host_config),
            ..Default::default()
        };

        let options: CreateContainerOptions = CreateContainerOptionsBuilder::new()
            .name(&self.service.name)
            .build();

        docker.create_container(Some(options), config).await?;

        // Connect to additional networks (first one is already connected via network_mode)
        for network in self.network.iter().skip(1) {
            let connect_request = NetworkConnectRequest {
                container: Some(self.service.name.clone()),
                endpoint_config: Some(EndpointSettings::default()),
            };

            docker
                .connect_network(&network.name, connect_request)
                .await
                .inspect_err(|e| {
                    log::warn!(
                        "Failed to connect container {} to network {}: {}",
                        self.service.name,
                        network.name,
                        e
                    );
                })?;
        }

        log::info!("Container {} created successfully", self.service.name);
        Ok(())
    }

    pub async fn recreate_container(&self) -> Result<(), ServiceConfigError> {
        self.pull_image().await?;
        let _ = self.stop_container().await;
        let _ = self.remove_container().await;
        self.create_container().await?;
        Ok(())
    }

    pub async fn container_does_not_exist(&self) -> bool {
        let docker = get_docker();

        let options: InspectContainerOptions = InspectContainerOptionsBuilder::new().build();

        match docker
            .inspect_container(&self.service.name, Some(options))
            .await
        {
            Ok(_) => false,
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => {
                log::info!(
                    "Container {} does not exist, needs creation",
                    self.service.name
                );
                true
            }
            Err(e) => {
                log::warn!("Failed to inspect container {}: {}", self.service.name, e);
                true // If we can't inspect, assume recreate is needed
            }
        }
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

/// Parse an image reference into (image, tag) components
fn parse_image_reference(image: &str) -> (&str, &str) {
    // Handle digest references (image@sha256:...)
    if let Some(at_pos) = image.find('@') {
        return (&image[..at_pos], &image[at_pos..]);
    }

    // Handle tag references (image:tag)
    // Need to be careful with registry URLs that contain port numbers
    // e.g., localhost:5000/myimage:tag
    if let Some(colon_pos) = image.rfind(':') {
        // Check if the colon is part of a port number in the registry URL
        let after_colon = &image[colon_pos + 1..];
        // If there's a slash after the colon, it's a port number, not a tag
        if !after_colon.contains('/') {
            return (&image[..colon_pos], after_colon);
        }
    }

    // No tag specified, use "latest"
    (image, "latest")
}

/// Parse memory limit string (e.g., "512m", "2g") to bytes
fn parse_memory_limit(limit: &str) -> i64 {
    let limit = limit.trim().to_lowercase();
    let (num_str, multiplier) = if limit.ends_with("g") {
        (&limit[..limit.len() - 1], 1024 * 1024 * 1024)
    } else if limit.ends_with("m") {
        (&limit[..limit.len() - 1], 1024 * 1024)
    } else if limit.ends_with("k") {
        (&limit[..limit.len() - 1], 1024)
    } else if limit.ends_with("b") {
        (&limit[..limit.len() - 1], 1)
    } else {
        (limit.as_str(), 1)
    };

    num_str.parse::<i64>().unwrap_or(0) * multiplier
}
