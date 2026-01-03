//! Network management module for Docker networks.
//!
//! This module provides functionality to manage Docker networks from the entrypoint configuration.
//! Networks are created before services start and can be cleaned up on shutdown.
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

use bollard::models::NetworkCreateRequest;
use bollard::query_parameters::{InspectNetworkOptions, InspectNetworkOptionsBuilder};

use crate::service::vars::ServiceConfigError;
use crate::service::{
    docker::get_docker,
    file::{NetworkDeclarationEntry, NetworkDriver},
};

pub struct NetworkInstance {
    pub name: String,
    pub driver: NetworkDriver,
    pub external: bool,
    pub internal: bool,
    pub attachable: bool,
    pub labels: HashMap<String, String>,
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
        }
    }
}

impl NetworkInstance {
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

        let request = NetworkCreateRequest {
            name: self.name.clone(),
            driver: Some(driver.to_string()),
            internal: Some(self.internal),
            attachable: Some(self.attachable),
            labels: Some(self.labels.clone()),
            ..Default::default()
        };

        match docker.create_network(request).await {
            Ok(_) => {
                log::info!("Network {} created successfully", self.name);
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
