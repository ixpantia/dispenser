use std::net::{Ipv4Addr, SocketAddrV4};
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};

use bollard::models::{
    ContainerCreateBody, EndpointIpamConfig, EndpointSettings, HealthStatusEnum, HostConfig,
    NetworkConnectRequest, NetworkingConfig, PortBinding, RestartPolicy, RestartPolicyNameEnum,
};
use bollard::query_parameters::{
    CreateContainerOptions, CreateContainerOptionsBuilder, CreateImageOptions,
    CreateImageOptionsBuilder, InspectContainerOptions, InspectContainerOptionsBuilder,
    RemoveContainerOptions, RemoveContainerOptionsBuilder, StartContainerOptions,
    StartContainerOptionsBuilder, StopContainerOptions, StopContainerOptionsBuilder,
};
use futures_util::StreamExt;

use crate::service::cron_watcher::CronWatcher;
use crate::service::file::ProxySettings;
use crate::service::vars::ServiceConfigError;
use crate::service::{
    docker::get_docker,
    file::{
        DependsOnCondition, DispenserConfig, Initialize, Network, PortEntry, PullOptions, Restart,
        ServiceEntry, VolumeEntry,
    },
    manifest::{ImageWatcher, ImageWatcherStatus},
    network::DEFAULT_NETWORK_NAME,
};

#[derive(Debug, PartialEq, Eq)]
pub struct ServiceInstanceConfig {
    pub dir: PathBuf,
    pub service: ServiceEntry,
    pub ports: Vec<PortEntry>,
    pub volume: Vec<VolumeEntry>,
    pub env: HashMap<String, String>,
    pub network: Vec<Network>,
    pub dispenser: DispenserConfig,
    pub depends_on: HashMap<String, DependsOnCondition>,
    pub proxy: Option<ProxySettings>,
    /// The static IP address assigned to this service on the dispenser network.
    /// This is managed by dispenser's IPAM to ensure stability across restarts.
    pub assigned_ip: Ipv4Addr,
}

#[derive(Debug)]
pub struct ServiceInstance {
    pub config: Arc<ServiceInstanceConfig>,
    pub cron_watcher: Option<CronWatcher>,
    pub image_watcher: Option<ImageWatcher>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    Starting,
    Healthy,
    Unhealthy,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerStatus {
    Running { health: HealthStatus },
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
                    let health = match state.health.and_then(|h| h.status) {
                        Some(HealthStatusEnum::HEALTHY) => HealthStatus::Healthy,
                        Some(HealthStatusEnum::UNHEALTHY) => HealthStatus::Unhealthy,
                        Some(HealthStatusEnum::STARTING) => HealthStatus::Starting,
                        _ => HealthStatus::None,
                    };
                    return Ok(ContainerStatus::Running { health });
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
        let mut depends_on_conditions = Vec::with_capacity(self.config.depends_on.len());
        loop {
            for (container, condition) in &self.config.depends_on {
                let status = match get_container_status(container).await {
                    Ok(status) => match condition {
                        DependsOnCondition::Started => {
                            matches!(status, ContainerStatus::Running { .. })
                        }
                        DependsOnCondition::Completed => {
                            matches!(status, ContainerStatus::Exited(0))
                        }
                        DependsOnCondition::Healthy => match status {
                            ContainerStatus::Running { health } => {
                                matches!(health, HealthStatus::Healthy | HealthStatus::None)
                            }
                            _ => false,
                        },
                    },
                    Err(_) => false,
                };
                if !status {
                    log::info!(
                        "Service {} is waiting for {} ({:?})",
                        self.config.service.name,
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

        if self.config.dispenser.pull == PullOptions::Always
            || self.container_does_not_exist().await
        {
            self.recreate_container().await?;
        }

        let docker = get_docker();

        let options: StartContainerOptions = StartContainerOptionsBuilder::new().build();

        docker
            .start_container(&self.config.service.name, Some(options))
            .await
            .inspect_err(|e| {
                log::error!(
                    "Failed to start container {}: {}",
                    self.config.service.name,
                    e
                );
            })?;

        log::info!(
            "Container {} started successfully",
            self.config.service.name
        );

        Ok(())
    }

    /// Get the socket address for this service if proxy is configured.
    /// The address is computed directly from the static IP and service port.
    pub fn get_socket_addr(&self) -> Option<SocketAddrV4> {
        self.config.proxy.as_ref().map(|proxy_settings| {
            SocketAddrV4::new(self.config.assigned_ip, proxy_settings.service_port)
        })
    }

    pub async fn pull_image(&self) -> Result<(), ServiceConfigError> {
        log::info!("Pulling image: {}", self.config.service.image);
        let docker = get_docker();

        // Parse image name and tag
        let (image, tag) =
            crate::service::docker::parse_image_reference(&self.config.service.image);
        let registry = crate::service::docker::extract_registry(image);
        let credentials = crate::service::docker::get_credentials(registry).await;

        let options: CreateImageOptions = CreateImageOptionsBuilder::new()
            .from_image(image)
            .tag(tag)
            .build();

        let mut stream = docker.create_image(Some(options), None, credentials);

        while let Some(result) = stream.next().await {
            match result {
                Ok(info) => {
                    if let Some(status) = info.status {
                        log::debug!("Pull status: {}", status);
                    }
                }
                Err(e) => {
                    log::error!("Failed to pull image {}: {}", self.config.service.image, e);
                    return Err(ServiceConfigError::DockerApi(e));
                }
            }
        }

        log::info!("Image {} pulled successfully", self.config.service.image);
        Ok(())
    }

    pub async fn stop_container(&self) -> Result<(), ServiceConfigError> {
        log::info!("Stopping container: {}", self.config.service.name);
        let docker = get_docker();

        let options: StopContainerOptions = StopContainerOptionsBuilder::new().t(10).build();

        match docker
            .stop_container(&self.config.service.name, Some(options))
            .await
        {
            Ok(_) => {
                log::info!(
                    "Container {} stopped successfully",
                    self.config.service.name
                );
                Ok(())
            }
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => {
                log::warn!(
                    "Container {} not found, skipping stop",
                    self.config.service.name
                );
                Ok(())
            }
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 304, ..
            }) => {
                log::info!("Container {} already stopped", self.config.service.name);
                Ok(())
            }
            Err(e) => {
                log::warn!(
                    "Failed to stop container {}: {}",
                    self.config.service.name,
                    e
                );
                Err(ServiceConfigError::DockerApi(e))
            }
        }
    }

    pub async fn remove_container(&self) -> Result<(), ServiceConfigError> {
        log::info!("Removing container: {}", self.config.service.name);
        let docker = get_docker();

        let options: RemoveContainerOptions =
            RemoveContainerOptionsBuilder::new().force(true).build();

        match docker
            .remove_container(&self.config.service.name, Some(options))
            .await
        {
            Ok(_) => {
                log::info!(
                    "Container {} removed successfully",
                    self.config.service.name
                );
                Ok(())
            }
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => {
                log::info!(
                    "Container {} not found, skipping removal",
                    self.config.service.name
                );
                Ok(())
            }
            Err(e) => {
                log::error!(
                    "Failed to remove container {}: {}",
                    self.config.service.name,
                    e
                );
                Err(ServiceConfigError::DockerApi(e))
            }
        }
    }

    pub async fn create_container(&self) -> Result<(), ServiceConfigError> {
        log::info!("Creating container: {}", self.config.service.name);
        let docker = get_docker();

        // Build port bindings
        let mut port_bindings: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();
        let mut exposed_ports: HashMap<String, HashMap<(), ()>> = HashMap::new();

        for port in &self.config.ports {
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
            .config
            .volume
            .iter()
            .map(|v| {
                let source = v.normalized_source(&self.config.dir)?;
                if v.readonly {
                    Ok(format!("{}:{}:ro", source, v.target))
                } else {
                    Ok(format!("{}:{}", source, v.target))
                }
            })
            .collect::<Result<_, ServiceConfigError>>()?;

        // Build environment variables
        let env: Vec<String> = self
            .config
            .env
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();

        // Build restart policy
        let restart_policy = match self.config.service.restart {
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
        let memory = self
            .config
            .service
            .memory
            .as_ref()
            .map(|m| parse_memory_limit(m));

        // Parse CPU limit (convert to nano CPUs)
        let nano_cpus = self.config.service.cpus.as_ref().map(|c| {
            let cpus: f64 = c.parse().unwrap_or(1.0);
            (cpus * 1_000_000_000.0) as i64
        });

        // Build host config
        // Always connect to the default dispenser network first
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
            network_mode: Some(DEFAULT_NETWORK_NAME.to_string()),
            ..Default::default()
        };

        // Build networking config to attach to the default dispenser network with static IP
        let mut endpoints_config: HashMap<String, EndpointSettings> = HashMap::new();
        endpoints_config.insert(
            DEFAULT_NETWORK_NAME.to_string(),
            EndpointSettings {
                ipam_config: Some(EndpointIpamConfig {
                    ipv4_address: Some(self.config.assigned_ip.to_string()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );

        let networking_config = NetworkingConfig {
            endpoints_config: Some(endpoints_config),
        };

        // Build container config
        let config = ContainerCreateBody {
            image: Some(self.config.service.image.clone()),
            hostname: self.config.service.hostname.clone(),
            user: self.config.service.user.clone(),
            working_dir: self.config.service.working_dir.clone(),
            env: if env.is_empty() { None } else { Some(env) },
            cmd: self.config.service.command.clone(),
            entrypoint: self.config.service.entrypoint.clone(),
            exposed_ports: if exposed_ports.is_empty() {
                None
            } else {
                Some(exposed_ports)
            },
            host_config: Some(host_config),
            networking_config: Some(networking_config),
            ..Default::default()
        };

        let options: CreateContainerOptions = CreateContainerOptionsBuilder::new()
            .name(&self.config.service.name)
            .build();

        docker.create_container(Some(options), config).await?;

        // Connect to user-defined networks (default dispenser network is already connected)
        for network in &self.config.network {
            let connect_request = NetworkConnectRequest {
                container: Some(self.config.service.name.clone()),
                endpoint_config: Some(EndpointSettings::default()),
            };

            docker
                .connect_network(&network.name, connect_request)
                .await
                .inspect_err(|e| {
                    log::warn!(
                        "Failed to connect container {} to network {}: {}",
                        self.config.service.name,
                        network.name,
                        e
                    );
                })?;
        }

        log::info!(
            "Container {} created successfully",
            self.config.service.name
        );
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
            .inspect_container(&self.config.service.name, Some(options))
            .await
        {
            Ok(_) => false,
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => {
                log::info!(
                    "Container {} does not exist, needs creation",
                    self.config.service.name
                );
                true
            }
            Err(e) => {
                log::warn!(
                    "Failed to inspect container {}: {}",
                    self.config.service.name,
                    e
                );
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
        self.config != other.config
    }

    pub async fn recreate_if_required(&self, other: &Self) {
        if self.requires_recreate(other).await {
            if let Err(e) = self.recreate_container().await {
                log::error!(
                    "Failed to recreate container {}: {}",
                    self.config.service.name,
                    e
                );
            }
        }
    }

    pub async fn poll(&self, poll_images: bool, init: bool) {
        if init && self.config.dispenser.initialize == Initialize::Immediately {
            log::info!("Starting {} immediately", self.config.service.name);
            if let Err(e) = self.run_container().await {
                log::error!(
                    "Failed to run container {}: {}",
                    self.config.service.name,
                    e
                );
            }
            return;
        }

        // If uses cron
        if let Some(cron_watcher) = &self.cron_watcher {
            if cron_watcher.is_ready() {
                // If the cron matches we can short circuit the function
                if let Err(e) = self.run_container().await {
                    log::error!(
                        "Failed to run container {} from cron: {}",
                        self.config.service.name,
                        e
                    );
                }

                return;
            }
        }

        // If its ready to poll images
        if self.config.dispenser.watch && poll_images {
            // try to update the watchers and check
            // if any of them were updated
            if let Some(image_watcher) = &self.image_watcher {
                match image_watcher.update().await {
                    ImageWatcherStatus::Updated => {
                        log::info!(
                            "Image updated for service {}, recreating container...",
                            self.config.service.name
                        );
                        if let Err(e) = self.recreate_container().await {
                            log::error!(
                                "Failed to recreate container {}: {}",
                                self.config.service.name,
                                e
                            );
                        }
                        if let Err(e) = self.run_container().await {
                            log::error!(
                                "Failed to run container {}: {}",
                                self.config.service.name,
                                e
                            );
                        }
                    }
                    ImageWatcherStatus::Deleted => {
                        log::warn!("Image for service {} was deleted", self.config.service.name);
                    }
                    ImageWatcherStatus::NotUpdated => {}
                }
            }
        }
    }
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
