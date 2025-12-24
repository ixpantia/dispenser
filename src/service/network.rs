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

use crate::service::file::{NetworkDeclarationEntry, NetworkDriver};

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
    /// Check if a network exists
    pub async fn check_network(&self) -> Result<NetworkStatus, std::io::Error> {
        let output = tokio::process::Command::new("docker")
            .args(["network", "inspect", &self.name])
            .output()
            .await?;

        if output.status.success() {
            Ok(NetworkStatus::Exists)
        } else {
            Ok(NetworkStatus::NotFound)
        }
    }

    /// Create the network if it doesn't exist
    pub async fn create_network(&self) -> Result<(), std::io::Error> {
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

        let mut cmd = tokio::process::Command::new("docker");
        cmd.args(["network", "create"]);

        // Add driver
        let driver_str = match self.driver {
            NetworkDriver::Bridge => "bridge",
            NetworkDriver::Host => "host",
            NetworkDriver::Overlay => "overlay",
            NetworkDriver::Macvlan => "macvlan",
            NetworkDriver::None => "none",
        };
        cmd.args(["--driver", driver_str]);

        // Add internal flag
        if self.internal {
            cmd.arg("--internal");
        }

        // Add attachable flag (useful for overlay networks)
        if self.attachable {
            cmd.arg("--attachable");
        }

        // Add labels
        for (key, value) in &self.labels {
            cmd.args(["--label", &format!("{}={}", key, value)]);
        }

        // Add the network name
        cmd.arg(&self.name);

        let output = cmd.output().await?;

        if output.status.success() {
            log::info!("Network {} created successfully", self.name);
            Ok(())
        } else {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            log::error!("Failed to create network {}: {}", self.name, error_msg);
            Err(std::io::Error::other(
                format!("Failed to create network: {}", error_msg),
            ))
        }
    }

    /// Remove the network
    pub async fn remove_network(&self) -> Result<(), std::io::Error> {
        // Don't remove external networks
        if self.external {
            log::info!(
                "Network {} is marked as external, skipping removal",
                self.name
            );
            return Ok(());
        }

        log::info!("Removing network: {}", self.name);

        let output = tokio::process::Command::new("docker")
            .args(["network", "rm", &self.name])
            .output()
            .await?;

        if output.status.success() {
            log::info!("Network {} removed successfully", self.name);
            Ok(())
        } else {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            log::warn!("Failed to remove network {}: {}", self.name, error_msg);
            // Don't return error for removal failures as they might be expected
            // (e.g., network still in use by containers)
            Ok(())
        }
    }

    /// Ensure the network exists (create if needed)
    pub async fn ensure_exists(&self) -> Result<(), std::io::Error> {
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
                    Err(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("External network {} not found", self.name),
                    ))
                } else {
                    self.create_network().await
                }
            }
        }
    }
}
