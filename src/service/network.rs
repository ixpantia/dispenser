//! Network management module for Docker networks.
//!
//! This module provides functionality to manage Docker networks from the entrypoint configuration.
//! Networks are created before services start and can be cleaned up on shutdown.
//!
//! # Default Network
//!
//! Dispenser automatically creates a default network (`dispenser`) that all containers
//! are connected to. This network uses a bridge driver with a specific subnet
//! (172.28.0.0/16) to provide predictable IP addresses for containers.
//!
//! # Example
//!
//! Networks are defined in the entrypoint file (e.g., `dispenser.toml`):
//!
//! ```toml
//! [[network]]
//! name = "app-network"
//! driver = "bridge"
//! internal = false
//! attachable = true
//!
//! [[network]]
//! name = "external-network"
//! driver = "bridge"
//! external = true  # Won't be created, must exist already
//! ```
//!
//! The `NetworkInstance` struct handles the creation, checking, and removal of networks.
//! Networks marked as `external = true` are expected to already exist and won't be created
//! or removed by the manager.

use std::collections::HashMap;

use bollard::models::{Ipam, IpamConfig, NetworkCreateRequest};
use bollard::query_parameters::{
    InspectContainerOptions, InspectContainerOptionsBuilder, InspectNetworkOptions,
    InspectNetworkOptionsBuilder,
};

use crate::service::vars::ServiceConfigError;
use crate::service::{
    docker::get_docker,
    file::{NetworkDeclarationEntry, NetworkDriver},
};

/// The name of the default dispenser network that all containers are connected to.
pub const DEFAULT_NETWORK_NAME: &str = "dispenser";

/// The subnet for the default dispenser network.
/// This provides a /16 network with 65,534 usable host addresses.
pub const DEFAULT_NETWORK_SUBNET: &str = "172.28.0.0/16";

/// The gateway IP for the default dispenser network.
pub const DEFAULT_NETWORK_GATEWAY: &str = "172.28.0.1";

pub struct NetworkInstance {
    pub name: String,
    pub driver: NetworkDriver,
    pub external: bool,
    pub internal: bool,
    pub attachable: bool,
    pub labels: HashMap<String, String>,
    /// Optional subnet configuration for the network (CIDR notation)
    pub subnet: Option<String>,
    /// Optional gateway IP for the network
    pub gateway: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkStatus {
    Exists,
    NotFound,
}

impl From<NetworkDeclarationEntry> for NetworkInstance {
    fn from(entry: NetworkDeclarationEntry) -> Self {
        Self {
            name: entry.name,
            driver: entry.driver,
            external: entry.external,
            internal: entry.internal,
            attachable: entry.attachable,
            labels: entry.labels,
            subnet: None,
            gateway: None,
        }
    }
}

impl NetworkInstance {
    /// Create the default dispenser network instance.
    /// This network is automatically created and all containers are connected to it.
    pub fn default_network() -> Self {
        let mut labels = HashMap::new();
        labels.insert("managed-by".to_string(), "dispenser".to_string());

        Self {
            name: DEFAULT_NETWORK_NAME.to_string(),
            driver: NetworkDriver::Bridge,
            external: false,
            internal: false,
            attachable: true,
            labels,
            subnet: Some(DEFAULT_NETWORK_SUBNET.to_string()),
            gateway: Some(DEFAULT_NETWORK_GATEWAY.to_string()),
        }
    }

    /// Check if a network exists using bollard
    pub async fn check_network(&self) -> Result<NetworkStatus, ServiceConfigError> {
        let docker = get_docker();

        let options: InspectNetworkOptions = InspectNetworkOptionsBuilder::new().build();

        match docker.inspect_network(&self.name, Some(options)).await {
            Ok(_) => Ok(NetworkStatus::Exists),
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => Ok(NetworkStatus::NotFound),
            Err(e) => Err(ServiceConfigError::DockerApi(e)),
        }
    }

    /// Create the network if it doesn't exist using bollard
    pub async fn create_network(&self) -> Result<(), ServiceConfigError> {
        // If external, we don't create it - it should already exist
        if self.external {
            log::info!(
                "Network {} is marked as external, skipping creation",
                self.name
            );
            return Ok(());
        }

        // Check if network already exists
        let status = self.check_network().await?;
        if status == NetworkStatus::Exists {
            log::info!("Network {} already exists, skipping creation", self.name);
            return Ok(());
        }

        log::info!("Creating network: {}", self.name);

        let docker = get_docker();

        let driver = match self.driver {
            NetworkDriver::Bridge => "bridge",
            NetworkDriver::Host => "host",
            NetworkDriver::Overlay => "overlay",
            NetworkDriver::Macvlan => "macvlan",
            NetworkDriver::None => "none",
        };

        // Build IPAM configuration if subnet is specified
        let ipam = if self.subnet.is_some() || self.gateway.is_some() {
            let ipam_config = IpamConfig {
                subnet: self.subnet.clone(),
                gateway: self.gateway.clone(),
                ip_range: None,
                auxiliary_addresses: None,
            };

            Some(Ipam {
                driver: Some("default".to_string()),
                config: Some(vec![ipam_config]),
                options: None,
            })
        } else {
            None
        };

        let request = NetworkCreateRequest {
            name: self.name.clone(),
            driver: Some(driver.to_string()),
            internal: Some(self.internal),
            attachable: Some(self.attachable),
            labels: Some(self.labels.clone()),
            ipam,
            ..Default::default()
        };

        match docker.create_network(request).await {
            Ok(_) => {
                log::info!("Network {} created successfully", self.name);
                if let Some(ref subnet) = self.subnet {
                    log::info!("  Subnet: {}", subnet);
                }
                if let Some(ref gateway) = self.gateway {
                    log::info!("  Gateway: {}", gateway);
                }
                Ok(())
            }
            Err(e) => {
                log::error!("Failed to create network {}: {}", self.name, e);
                Err(ServiceConfigError::DockerApi(e))
            }
        }
    }

    /// Remove the network using bollard
    pub async fn remove_network(&self) -> Result<(), ServiceConfigError> {
        // Don't remove external networks
        if self.external {
            log::info!(
                "Network {} is marked as external, skipping removal",
                self.name
            );
            return Ok(());
        }

        log::info!("Removing network: {}", self.name);

        let docker = get_docker();

        match docker.remove_network(&self.name).await {
            Ok(_) => {
                log::info!("Network {} removed successfully", self.name);
                Ok(())
            }
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => {
                log::info!("Network {} not found, skipping removal", self.name);
                Ok(())
            }
            Err(e) => {
                log::warn!("Failed to remove network {}: {}", self.name, e);
                // Don't return error for removal failures as they might be expected
                // (e.g., network still in use by containers)
                Ok(())
            }
        }
    }

    /// Ensure the network exists (create if needed)
    pub async fn ensure_exists(&self) -> Result<(), ServiceConfigError> {
        let status = self.check_network().await?;

        match status {
            NetworkStatus::Exists => {
                log::debug!("Network {} already exists", self.name);
                Ok(())
            }
            NetworkStatus::NotFound => {
                if self.external {
                    log::error!(
                        "External network {} does not exist. Please create it manually.",
                        self.name
                    );
                    Err(ServiceConfigError::NetworkNotFound(self.name.clone()))
                } else {
                    self.create_network().await
                }
            }
        }
    }
}

/// Ensure the default dispenser network exists.
/// This should be called during manager initialization before any containers are created.
pub async fn ensure_default_network() -> Result<(), ServiceConfigError> {
    let default_network = NetworkInstance::default_network();
    default_network.ensure_exists().await
}

/// Remove the default dispenser network.
/// This should be called during shutdown after all containers have been removed.
pub async fn remove_default_network() -> Result<(), ServiceConfigError> {
    let default_network = NetworkInstance::default_network();
    default_network.remove_network().await
}

/// Get the IP address of a container on the default dispenser network.
///
/// Returns `None` if the container is not found or not connected to the dispenser network.
///
/// # Arguments
///
/// * `container_name` - The name of the container to get the IP address for.
///
/// # Example
///
/// ```rust,ignore
/// if let Some(ip) = get_container_ip("my-app").await? {
///     println!("Container IP: {}", ip);
/// }
/// ```
pub async fn get_container_ip(container_name: &str) -> Result<Option<String>, ServiceConfigError> {
    let docker = get_docker();

    let options: InspectContainerOptions = InspectContainerOptionsBuilder::new().build();

    match docker
        .inspect_container(container_name, Some(options))
        .await
    {
        Ok(info) => {
            if let Some(network_settings) = info.network_settings {
                if let Some(networks) = network_settings.networks {
                    if let Some(dispenser_network) = networks.get(DEFAULT_NETWORK_NAME) {
                        return Ok(dispenser_network.ip_address.clone());
                    }
                }
            }
            Ok(None)
        }
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 404, ..
        }) => Ok(None),
        Err(e) => Err(ServiceConfigError::DockerApi(e)),
    }
}

/// Get all container IP addresses on the default dispenser network.
///
/// Returns a map of container name to IP address for all containers
/// connected to the dispenser network.
pub async fn get_all_container_ips() -> Result<HashMap<String, String>, ServiceConfigError> {
    let docker = get_docker();

    let options: InspectNetworkOptions = InspectNetworkOptionsBuilder::new().build();

    match docker
        .inspect_network(DEFAULT_NETWORK_NAME, Some(options))
        .await
    {
        Ok(network) => {
            let mut ips = HashMap::new();

            if let Some(containers) = network.containers {
                for (_, container_info) in containers {
                    if let (Some(name), Some(ip)) =
                        (container_info.name, container_info.ipv4_address)
                    {
                        // Remove the CIDR suffix if present (e.g., "172.28.0.2/16" -> "172.28.0.2")
                        let ip_only = ip.split('/').next().unwrap_or(&ip).to_string();
                        ips.insert(name, ip_only);
                    }
                }
            }

            Ok(ips)
        }
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 404, ..
        }) => Ok(HashMap::new()),
        Err(e) => Err(ServiceConfigError::DockerApi(e)),
    }
}
