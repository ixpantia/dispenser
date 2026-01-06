use cron::Schedule;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use super::vars::{render_template, ServiceConfigError, ServiceVarsMaterialized};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntrypointFile {
    #[serde(rename = "service", default)]
    pub services: Vec<EntrypointFileEntry>,
    #[serde(rename = "network", default)]
    pub networks: Vec<NetworkDeclarationEntry>,
    #[serde(default)]
    pub proxy: GlobalProxyConfig,
    /// Delay in seconds between polling for new images (default: 60)
    #[serde(default = "default_delay")]
    pub delay: u64,
    pub certbot: Option<CertbotSettings>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum ProxyStrategy {
    #[serde(alias = "https-only", alias = "HttpsOnly")]
    #[default]
    HttpsOnly,
    #[serde(alias = "http-only", alias = "HttpOnly")]
    HttpOnly,
    #[serde(alias = "both", alias = "Both")]
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlobalProxyConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub strategy: ProxyStrategy,
}

impl Default for GlobalProxyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            strategy: ProxyStrategy::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CertbotSettings {
    pub email: String,
}

fn default_delay() -> u64 {
    60
}

impl EntrypointFile {
    pub async fn try_init(vars: &ServiceVarsMaterialized) -> Result<Self, ServiceConfigError> {
        let path = crate::cli::get_cli_args().config.clone();
        let config = tokio::fs::read_to_string(&path).await?;

        // Render the template with variables
        let rendered_config =
            render_template(&config, vars).map_err(|e| ServiceConfigError::Template((path, e)))?;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProxySettings {
    /// Example: example.com, something.dispenser.org
    ///
    /// Equivalent to nginx server_name but without wildcards.
    ///
    /// TODO: Could we choose a better name?
    ///
    /// TODO: Document this
    pub host: String,
    pub path: Option<String>,
    /// The port of the service running inside the container.
    /// The dispenser reverse proxy will send HTTP/WebSocket traffic
    /// to this port.
    ///
    /// TODO: Can we have a better name for this config value?
    pub service_port: u16,
    pub cert_file: Option<PathBuf>,
    pub key_file: Option<PathBuf>,
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
    #[serde(default)]
    pub proxy: Option<ProxySettings>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum PullOptions {
    #[serde(alias = "always")]
    Always,
    #[default]
    #[serde(alias = "on-startup", alias = "on_startup", alias = "onstartup")]
    OnStartup,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DispenserConfig {
    pub watch: bool,
    #[serde(default)]
    pub initialize: Initialize,
    pub cron: Option<Schedule>,
    #[serde(default)]
    pub pull: PullOptions,
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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum VolumeSource {
    Name(String),
    Path(PathBuf),
}

impl<'de> Deserialize<'de> for VolumeSource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        if raw.contains('/') {
            return Ok(Self::Path(PathBuf::from(raw)));
        }
        Ok(Self::Name(raw))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VolumeEntry {
    pub source: VolumeSource,
    pub target: String,
    #[serde(default)]
    pub readonly: bool,
}

impl VolumeEntry {
    // If the source is a path, returns the
    // absolute path to the path entry relative to
    // the `service.toml` file. If it's a volume name
    // it returns the volume name directly.
    pub fn normalized_source(&self, wd: &Path) -> Result<String, ServiceConfigError> {
        // Since this type is just a string behind the scenes
        // we can unwrap and guarantee utf-8
        match &self.source {
            VolumeSource::Path(path) => {
                if Path::new(path).is_absolute() {
                    return Ok(String::from_utf8(
                        path.clone().into_os_string().into_encoded_bytes(),
                    )?);
                }
                Ok(String::from_utf8(
                    std::path::absolute(wd.join(path))?
                        .into_os_string()
                        .into_encoded_bytes(),
                )?)
            }
            VolumeSource::Name(name) => Ok(name.clone()),
        }
    }
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
