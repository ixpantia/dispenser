use cron::Schedule;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

use super::vars::{render_template, ServiceConfigError, ServiceVarsMaterialized};

#[derive(Debug, Serialize, Deserialize)]
pub struct EntrypointFile {
    pub services: Vec<EntrypointFileEntry>,
}

impl EntrypointFile {
    pub async fn try_init() -> Result<Self, ServiceConfigError> {
        use std::io::Read;
        let mut config = String::new();
        std::fs::File::open(&crate::cli::get_cli_args().config)?.read_to_string(&mut config)?;

        // Load and materialize variables
        let vars = ServiceVarsMaterialized::try_init().await?;

        // Render the template with variables
        let rendered_config = render_template(&config, &vars)?;

        // Parse the rendered config as TOML
        Ok(toml::from_str(&rendered_config)?)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EntrypointFileEntry {
    /// Path to the directory where a service.toml file is found.
    /// This toml file should be deserialized into a ServiceFile.
    /// This path is relative to the location of EntrypointFile.
    pub path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub enum DependsOnCondition {
    ServiceStarted,
    ServiceCompleted,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DispenserConfig {
    pub watch: bool,
    #[serde(default)]
    pub initialize: Initialize,
    pub cron: Option<Schedule>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Network {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct PortEntry {
    pub host: u16,
    pub container: u16,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VolumeEntry {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ServiceEntry {
    pub name: String,
    pub image: String,
    /// Memory limit (e.g., "512m", "2g")
    pub memory: Option<String>,
    /// Number of CPUs (e.g., "1.5", "2")
    pub cpus: Option<String>,
}
