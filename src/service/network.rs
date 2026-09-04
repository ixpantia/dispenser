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

use std::collections::{HashMap, HashSet};
use std::net::Ipv4Addr;

use bollard::models::{Ipam, IpamConfig, NetworkCreateRequest};
use bollard::query_parameters::{InspectNetworkOptions, InspectNetworkOptionsBuilder};

use crate::service::vars::ServiceConfigError;
use crate::service::{
    docker::get_docker,
    file::{DefaultNetworkConfig, NetworkDeclarationEntry, NetworkDriver},
};

/// The name of the default dispenser network that all containers are connected to.
pub const DEFAULT_NETWORK_NAME: &str = "dispenser";

/// The subnet for the default dispenser network.
/// This provides a /16 network with 65,534 usable host addresses.
pub const DEFAULT_NETWORK_SUBNET: &str = "172.28.0.0/16";

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
    /// Create the default dispenser network instance with default settings.
    /// This network is automatically created and all containers are connected to it.
    /// Panics if the default configuration is invalid (should never happen).
    pub fn default_network() -> Self {
        Self::default_network_with_config(DefaultNetworkConfig::default())
            .expect("Default network configuration should always be valid")
    }

    /// Create the default dispenser network instance with custom configuration.
    /// This network is automatically created and all containers are connected to it.
    /// Returns an error if the subnet configuration is invalid.
    pub fn default_network_with_config(
        config: DefaultNetworkConfig,
    ) -> Result<Self, ServiceConfigError> {
        let mut labels = HashMap::new();
        labels.insert("managed-by".to_string(), "dispenser".to_string());

        let subnet = config
            .subnet
            .unwrap_or_else(|| DEFAULT_NETWORK_SUBNET.to_string());
        let gateway = match config.gateway {
            Some(g) => g,
            None => {
                // Default gateway is the first IP in the subnet
                // For a subnet like "172.28.0.0/16", the gateway would be "172.28.0.1"
                derive_gateway_from_subnet(&subnet)?
            }
        };

        Ok(Self {
            name: DEFAULT_NETWORK_NAME.to_string(),
            driver: NetworkDriver::Bridge,
            external: false,
            internal: false,
            attachable: true,
            labels,
            subnet: Some(subnet),
            gateway: Some(gateway),
        })
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

/// Derive the gateway IP from a subnet CIDR string.
/// For a subnet like "172.28.0.0/16", the gateway would be "172.28.0.1".
/// For "10.10.0.0/24", it would be "10.10.0.1".
/// Returns an error if the subnet is not valid CIDR notation.
fn derive_gateway_from_subnet(subnet: &str) -> Result<String, ServiceConfigError> {
    // Parse the CIDR notation (e.g., "172.28.0.0/16")
    let parts: Vec<&str> = subnet.split('/').collect();
    if parts.len() != 2 {
        return Err(ServiceConfigError::Config(format!(
            "Invalid subnet CIDR notation '{}': expected format 'X.X.X.X/Y'",
            subnet
        )));
    }

    // Validate the prefix part (must be a valid number between 0 and 32)
    let prefix: u8 = parts[1].parse::<u8>().map_err(|_| {
        ServiceConfigError::Config(format!(
            "Invalid prefix '{}' in subnet '{}': expected a number between 0 and 32",
            parts[1], subnet
        ))
    })?;
    if prefix > 32 {
        return Err(ServiceConfigError::Config(format!(
            "Invalid prefix '{}' in subnet '{}': must be between 0 and 32",
            parts[1], subnet
        )));
    }

    // Parse the IP part
    let ip_parts: Vec<&str> = parts[0].split('.').collect();
    if ip_parts.len() != 4 {
        return Err(ServiceConfigError::Config(format!(
            "Invalid IP address in subnet '{}': expected format 'X.X.X.X/Y'",
            subnet
        )));
    }

    // Validate that each octet is a valid number
    for (i, part) in ip_parts.iter().enumerate() {
        if part.parse::<u8>().is_err() {
            return Err(ServiceConfigError::Config(format!(
                "Invalid octet '{}' in subnet '{}'",
                ip_parts[i], subnet
            )));
        }
    }

    // The gateway is the first usable IP (network address + 1)
    Ok(format!("{}.{}.{}.1", ip_parts[0], ip_parts[1], ip_parts[2]))
}

/// Ensure the default dispenser network exists with default settings.
/// This should be called during manager initialization before any containers are created.
#[allow(dead_code)]
pub async fn ensure_default_network() -> Result<(), ServiceConfigError> {
    let default_network = NetworkInstance::default_network();
    default_network.ensure_exists().await
}

/// Ensure the default dispenser network exists with custom configuration.
/// This should be called during manager initialization before any containers are created.
/// If the network already exists with a different subnet, a warning is logged.
pub async fn ensure_default_network_with_config(
    config: DefaultNetworkConfig,
) -> Result<(), ServiceConfigError> {
    let default_network = NetworkInstance::default_network_with_config(config.clone())?;

    // Check if network already exists
    let docker = get_docker();
    let options: InspectNetworkOptions = InspectNetworkOptionsBuilder::new().build();

    match docker
        .inspect_network(DEFAULT_NETWORK_NAME, Some(options))
        .await
    {
        Ok(network_info) => {
            // Network exists - check if subnet matches configuration
            let existing_subnet = network_info
                .ipam
                .and_then(|ipam| ipam.config)
                .and_then(|configs| configs.into_iter().next())
                .and_then(|c| c.subnet);

            if let (Some(existing), Some(desired)) = (&existing_subnet, &config.subnet) {
                if existing != desired {
                    log::warn!(
                        "Default network '{}' exists with subnet {} but config specifies {}. \
                         Subnet change requires a full restart (stop dispenser and start again). \
                         Continuing with existing network configuration.",
                        DEFAULT_NETWORK_NAME,
                        existing,
                        desired
                    );
                }
            }

            log::debug!("Default network {} already exists", DEFAULT_NETWORK_NAME);
            Ok(())
        }
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 404, ..
        }) => {
            // Network doesn't exist, create it
            default_network.create_network().await
        }
        Err(e) => Err(ServiceConfigError::DockerApi(e)),
    }
}

/// Remove the default dispenser network.
/// This should be called during shutdown after all containers have been removed.
pub async fn remove_default_network() -> Result<(), ServiceConfigError> {
    let default_network = NetworkInstance::default_network();
    default_network.remove_network().await
}

/// Get the set of IP addresses currently in use by containers on the dispenser network.
/// This queries Docker's IPAM to find IPs that are actually allocated, including to
/// stopped containers that still exist.
///
/// This is essential for avoiding "Address already in use" errors during reload,
/// because Docker's IPAM considers IPs in use even by stopped containers.
pub async fn get_used_ips() -> Result<HashSet<Ipv4Addr>, ServiceConfigError> {
    let docker = get_docker();
    let options: InspectNetworkOptions = InspectNetworkOptionsBuilder::new().build();

    match docker
        .inspect_network(DEFAULT_NETWORK_NAME, Some(options))
        .await
    {
        Ok(network_info) => {
            let mut used_ips = HashSet::new();

            // Extract IPs from containers connected to the network
            if let Some(containers) = network_info.containers {
                for (_container_id, endpoint) in containers {
                    if let Some(ipv4_address) = endpoint.ipv4_address {
                        // Parse the IP address (may include CIDR suffix, e.g., "172.28.0.2/16")
                        let ip_str = ipv4_address.split('/').next().unwrap_or(&ipv4_address);
                        if let Ok(ip) = ip_str.parse::<Ipv4Addr>() {
                            used_ips.insert(ip);
                            log::debug!("Found IP {} in use by container on dispenser network", ip);
                        }
                    }
                }
            }

            log::info!("Found {} IPs in use on dispenser network", used_ips.len());
            Ok(used_ips)
        }
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 404, ..
        }) => {
            // Network doesn't exist yet, no IPs in use
            log::debug!("Dispenser network does not exist, no IPs in use");
            Ok(HashSet::new())
        }
        Err(e) => {
            log::error!("Failed to inspect dispenser network: {}", e);
            Err(ServiceConfigError::DockerApi(e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const DEFAULT_NETWORK_GATEWAY: &str = "172.28.0.1";

    #[test]
    fn test_default_network() {
        let network = NetworkInstance::default_network();

        assert_eq!(network.name, DEFAULT_NETWORK_NAME);
        assert_eq!(network.driver, NetworkDriver::Bridge);
        assert!(!network.external);
        assert!(!network.internal);
        assert!(network.attachable);
        assert_eq!(network.subnet, Some(DEFAULT_NETWORK_SUBNET.to_string()));
        assert_eq!(network.gateway, Some(DEFAULT_NETWORK_GATEWAY.to_string()));

        assert_eq!(
            network.labels.get("managed-by").map(|s| s.as_str()),
            Some("dispenser")
        );
    }

    #[test]
    fn test_network_instance_from_declaration() {
        let mut labels = HashMap::new();
        labels.insert("custom-label".to_string(), "value".to_string());

        let declaration = NetworkDeclarationEntry {
            name: "test-network".to_string(),
            driver: NetworkDriver::Overlay,
            external: true,
            internal: true,
            attachable: false,
            labels: labels.clone(),
        };

        let network = NetworkInstance::from(declaration);

        assert_eq!(network.name, "test-network");
        assert_eq!(network.driver, NetworkDriver::Overlay);
        assert!(network.external);
        assert!(network.internal);
        assert!(!network.attachable);
        assert_eq!(network.labels, labels);
        assert_eq!(network.subnet, None);
        assert_eq!(network.gateway, None);
    }

    #[test]
    fn test_derive_gateway_from_subnet() {
        assert_eq!(
            derive_gateway_from_subnet("172.28.0.0/16").unwrap(),
            "172.28.0.1"
        );
        assert_eq!(
            derive_gateway_from_subnet("10.10.0.0/24").unwrap(),
            "10.10.0.1"
        );
        assert_eq!(
            derive_gateway_from_subnet("192.168.1.0/24").unwrap(),
            "192.168.1.1"
        );
    }

    #[test]
    fn test_derive_gateway_from_subnet_invalid() {
        // Invalid CIDR should return an error
        assert!(derive_gateway_from_subnet("invalid").is_err());
        assert!(derive_gateway_from_subnet("172.28.0.0").is_err()); // Missing /prefix
        assert!(derive_gateway_from_subnet("172.28.0.0/").is_err()); // Empty prefix
        assert!(derive_gateway_from_subnet("/16").is_err()); // Missing IP
        assert!(derive_gateway_from_subnet("172.28.0/16").is_err()); // Only 3 octets
        assert!(derive_gateway_from_subnet("172.28.0.0.1/16").is_err()); // 5 octets
        assert!(derive_gateway_from_subnet("172.28.0.256/16").is_err()); // Invalid octet > 255
    }

    #[test]
    fn test_default_network_with_config() {
        let config = DefaultNetworkConfig {
            subnet: Some("10.10.0.0/16".to_string()),
            gateway: Some("10.10.0.254".to_string()),
        };

        let network = NetworkInstance::default_network_with_config(config).unwrap();

        assert_eq!(network.name, DEFAULT_NETWORK_NAME);
        assert_eq!(network.subnet, Some("10.10.0.0/16".to_string()));
        assert_eq!(network.gateway, Some("10.10.0.254".to_string()));
    }

    #[test]
    fn test_default_network_with_config_subnet_only() {
        let config = DefaultNetworkConfig {
            subnet: Some("10.20.0.0/16".to_string()),
            gateway: None,
        };

        let network = NetworkInstance::default_network_with_config(config).unwrap();

        assert_eq!(network.name, DEFAULT_NETWORK_NAME);
        assert_eq!(network.subnet, Some("10.20.0.0/16".to_string()));
        // Gateway should be derived from subnet
        assert_eq!(network.gateway, Some("10.20.0.1".to_string()));
    }

    #[test]
    fn test_default_network_with_empty_config() {
        let config = DefaultNetworkConfig::default();

        let network = NetworkInstance::default_network_with_config(config).unwrap();

        assert_eq!(network.name, DEFAULT_NETWORK_NAME);
        assert_eq!(network.subnet, Some(DEFAULT_NETWORK_SUBNET.to_string()));
        assert_eq!(network.gateway, Some(DEFAULT_NETWORK_GATEWAY.to_string()));
    }

    #[test]
    fn test_default_network_with_invalid_subnet() {
        let config = DefaultNetworkConfig {
            subnet: Some("invalid".to_string()),
            gateway: None,
        };

        // Should return an error for invalid subnet
        assert!(NetworkInstance::default_network_with_config(config).is_err());
    }
}
