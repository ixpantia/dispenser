use cron::Schedule;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

use super::vars::{render_template, ServiceConfigError, ServiceVarsMaterialized};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntrypointFile {
    #[serde(rename = "service")]
    pub services: Vec<EntrypointFileEntry>,
    #[serde(rename = "network")]
    pub networks: Vec<NetworkDeclarationEntry>,
    /// Delay in seconds between polling for new images (default: 60)
    #[serde(default = "default_delay")]
    pub delay: u64,
}

fn default_delay() -> u64 {
    60
}

impl EntrypointFile {
    pub async fn try_init(vars: &ServiceVarsMaterialized) -> Result<Self, ServiceConfigError> {
        use std::io::Read;
        let mut config = String::new();
        let path = crate::cli::get_cli_args().config.clone();
        std::fs::File::open(&path)?.read_to_string(&mut config)?;

        // Render the template with variables
        let rendered_config =
            render_template(&config, &vars).map_err(|e| ServiceConfigError::Template((path, e)))?;

        // Parse the rendered config as TOML
        Ok(toml::from_str(&rendered_config)?)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkDeclarationEntry {
    pub name: String,
    #[serde(default = "default_network_driver")]
    pub driver: NetworkDriver,
    #[serde(default = "default_false")]
    pub external: bool,
    #[serde(default = "default_false")]
    pub internal: bool,
    #[serde(default = "default_true")]
    pub attachable: bool,
    #[serde(default)]
    pub labels: HashMap<String, String>,
}

fn default_network_driver() -> NetworkDriver {
    NetworkDriver::Bridge
}

fn default_false() -> bool {
    false
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum NetworkDriver {
    #[default]
    #[serde(alias = "bridge")]
    Bridge,
    #[serde(alias = "host")]
    Host,
    #[serde(alias = "overlay")]
    Overlay,
    #[serde(alias = "macvlan")]
    Macvlan,
    #[serde(alias = "none")]
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntrypointFileEntry {
    /// Path to the directory where a service.toml file is found.
    /// This toml file should be deserialized into a ServiceFile.
    /// This path is relative to the location of EntrypointFile.
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceFile {
    pub service: ServiceEntry,
    #[serde(default, rename = "port")]
    pub ports: Vec<PortEntry>,
    #[serde(default, rename = "volume")]
    pub volume: Vec<VolumeEntry>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub restart: Restart,
    #[serde(default)]
    pub network: Vec<Network>,
    pub dispenser: DispenserConfig,
    #[serde(default)]
    pub depends_on: HashMap<String, DependsOnCondition>,
}

/// Defines when a service should be initialized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Initialize {
    /// The service is started as soon as the application starts.
    #[serde(alias = "immediately", alias = "Immediately")]
    #[default]
    Immediately,
    /// The service is started only when a trigger occurs (e.g., a cron schedule or a detected image update).
    #[serde(
        alias = "on-trigger",
        alias = "OnTrigger",
        alias = "on_trigger",
        alias = "on trigger"
    )]
    OnTrigger,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DependsOnCondition {
    #[serde(
        alias = "service-started",
        alias = "service_started",
        alias = "started"
    )]
    ServiceStarted,
    #[serde(
        alias = "service-completed",
        alias = "service_completed",
        alias = "completed"
    )]
    ServiceCompleted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DispenserConfig {
    pub watch: bool,
    #[serde(default)]
    pub initialize: Initialize,
    pub cron: Option<Schedule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Network {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum Restart {
    #[serde(alias = "always")]
    Always,
    #[default]
    #[serde(alias = "no", alias = "never")]
    No,
    #[serde(alias = "on-failure", alias = "on_failure", alias = "onfailure")]
    OnFailure,
    #[serde(
        alias = "unless-stopped",
        alias = "unless_stopped",
        alias = "unlessstopped"
    )]
    UnlessStopped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PortEntry {
    pub host: u16,
    pub container: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VolumeEntry {
    pub source: String,
    pub target: String,
    #[serde(default)]
    pub readonly: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceEntry {
    pub name: String,
    pub image: String,
    #[serde(default)]
    pub command: Option<Vec<String>>,
    #[serde(default)]
    pub entrypoint: Option<Vec<String>>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub hostname: Option<String>,
    /// Memory limit (e.g., "512m", "2g")
    pub memory: Option<String>,
    /// Number of CPUs (e.g., "1.5", "2")
    pub cpus: Option<String>,
}
