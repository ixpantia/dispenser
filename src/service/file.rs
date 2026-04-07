use cron::Schedule;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use url::Url;

use super::vars::{render_template, ServiceConfigError, ServiceVarsMaterialized};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
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
    pub telemetry: Option<TelemetryConfig>,
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
#[serde(deny_unknown_fields)]
pub struct GlobalProxyConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub strategy: ProxyStrategy,
    #[serde(default = "default_false")]
    pub trust_forwarded_headers: bool,
}

impl Default for GlobalProxyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            strategy: ProxyStrategy::default(),
            trust_forwarded_headers: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CertbotSettings {
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelemetryConfig {
    pub enabled: bool,
    #[serde(deserialize_with = "deserialize_base_uri")]
    pub base_uri: Url,
    pub buffer_size: Option<usize>,
    #[serde(default = "default_status_interval")]
    pub status_interval: u64,
}

/// Deserialize `base_uri` from a string, supporting:
/// - Cloud storage URIs (`s3://bucket/path`, `gs://...`, `az://...`, `file://...`)
/// - Absolute local paths (`/home/user/data`) → `file:///home/user/data`
/// - Relative paths (`telem`, `./data`) → resolved against CWD to `file://` URL
fn deserialize_base_uri<'de, D>(deserializer: D) -> Result<Url, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    let trimmed = raw.trim_end_matches('/');

    // Try parsing as a URL first — succeeds for schemes like s3://, gs://, az://, file://
    if let Ok(url) = Url::parse(trimmed) {
        return Ok(url);
    }

    // Not a valid URL, treat as a filesystem path
    let path = std::path::Path::new(trimmed);
    let abs_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::path::absolute(path).map_err(|e| {
            serde::de::Error::custom(format!(
                "Failed to resolve relative path '{}': {}",
                trimmed, e
            ))
        })?
    };

    Url::from_file_path(&abs_path).map_err(|_| {
        serde::de::Error::custom(format!(
            "Failed to convert path to URL: '{}'",
            abs_path.display()
        ))
    })
}

impl TelemetryConfig {
    fn table_url(&self, table_name: &str) -> Url {
        let mut url = self.base_uri.clone();
        // Ensure the path ends with '/' so we can append a segment
        if !url.path().ends_with('/') {
            url.set_path(&format!("{}/", url.path()));
        }
        url.join(table_name)
            .unwrap_or_else(|e| panic!("Failed to join table '{}': {}", table_name, e))
    }

    pub fn table_uri_deployments(&self) -> Url {
        self.table_url("deployments")
    }
    pub fn table_uri_status(&self) -> Url {
        self.table_url("status")
    }
    pub fn table_uri_logs(&self) -> Url {
        self.table_url("logs")
    }
    pub fn table_uri_traces(&self) -> Url {
        self.table_url("traces")
    }
    pub fn table_uri_container_output(&self) -> Url {
        self.table_url("container-output")
    }
}

fn default_status_interval() -> u64 {
    60
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct EntrypointFileEntry {
    /// Path to the directory where a service.toml file is found.
    /// This toml file should be deserialized into a ServiceFile.
    /// This path is relative to the location of EntrypointFile.
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ServiceFile {
    pub service: ServiceEntry,
    #[serde(default, rename = "port")]
    pub ports: Vec<PortEntry>,
    #[serde(default, rename = "volume")]
    pub volume: Vec<VolumeEntry>,
    #[serde(default)]
    pub env: HashMap<String, String>,
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
    Started,
    #[serde(
        alias = "service-completed",
        alias = "service_completed",
        alias = "completed"
    )]
    Completed,
    #[serde(
        alias = "service-healthy",
        alias = "service_healthy",
        alias = "healthy"
    )]
    Healthy,
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
#[serde(deny_unknown_fields)]
pub struct DispenserConfig {
    pub watch: bool,
    #[serde(default)]
    pub initialize: Initialize,
    pub cron: Option<Schedule>,
    #[serde(default)]
    pub pull: PullOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Network {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq, Copy)]
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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

/// Extract the registry part from an image name.
pub fn extract_registry(image: &str) -> &str {
    if let Some(slash_pos) = image.find('/') {
        let part = &image[..slash_pos];
        // If the first part contains a dot or colon, or is "localhost", it's a registry
        if part.contains('.') || part.contains(':') || part == "localhost" {
            return part;
        }
    }
    "docker.io"
}

/// Parse an image reference into (image, tag) components
pub fn parse_image_reference(image: &str) -> Image {
    let registry = extract_registry(image);

    // Handle digest references (image@sha256:...)
    if let Some(at_pos) = image.find('@') {
        let (name, tag) = (&image[..at_pos], &image[at_pos..]);
        return Image {
            registry: registry.into(),
            name: name.into(),
            tag: tag.into(),
        };
    }

    // Handle tag references (image:tag)
    // Need to be careful with registry URLs that contain port numbers
    // e.g., localhost:5000/myimage:tag
    if let Some(colon_pos) = image.rfind(':') {
        // Check if the colon is part of a port number in the registry URL
        let after_colon = &image[colon_pos + 1..];
        // If there's a slash after the colon, it's a port number, not a tag
        if !after_colon.contains('/') {
            let (name, tag) = (&image[..colon_pos], after_colon);
            return Image {
                registry: registry.into(),
                name: name.into(),
                tag: tag.into(),
            };
        }
    }

    // No tag specified, use "latest"
    return Image {
        registry: registry.into(),
        name: image.into(),
        tag: "latest".into(),
    };
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Image {
    pub name: Box<str>,
    pub registry: Box<str>,
    pub tag: Box<str>,
}

impl<'de> serde::de::Deserialize<'de> for Image {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let image_str = String::deserialize(deserializer)?;

        Ok(parse_image_reference(&image_str))
    }
}

impl serde::ser::Serialize for Image {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        str::serialize(&self.to_string(), serializer)
    }
}

impl std::fmt::Display for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.tag.starts_with('@') {
            write!(f, "{}{}", self.name, self.tag)
        } else {
            write!(f, "{}:{}", self.name, self.tag)
        }
    }
}

impl From<&str> for Image {
    fn from(s: &str) -> Self {
        parse_image_reference(s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ServiceEntry {
    pub name: String,
    pub image: Image,
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
    #[serde(default)]
    pub restart: Restart,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volume_source_deserialize() {
        let path_json = r#""/absolute/path""#;
        let source: VolumeSource = serde_json::from_str(path_json).unwrap();
        assert_eq!(source, VolumeSource::Path(PathBuf::from("/absolute/path")));

        let rel_path_json = r#""./relative/path""#;
        let source: VolumeSource = serde_json::from_str(rel_path_json).unwrap();
        assert_eq!(source, VolumeSource::Path(PathBuf::from("./relative/path")));

        let name_json = r#""my_volume""#;
        let source: VolumeSource = serde_json::from_str(name_json).unwrap();
        assert_eq!(source, VolumeSource::Name("my_volume".to_string()));
    }

    #[test]
    fn test_volume_entry_normalized_source() {
        let wd = Path::new("/tmp");

        let entry_abs = VolumeEntry {
            source: VolumeSource::Path(PathBuf::from("/absolute/path")),
            target: "/target".to_string(),
            readonly: false,
        };
        assert_eq!(entry_abs.normalized_source(wd).unwrap(), "/absolute/path");

        let entry_rel = VolumeEntry {
            source: VolumeSource::Path(PathBuf::from("relative/path")),
            target: "/target".to_string(),
            readonly: false,
        };
        let norm = entry_rel.normalized_source(wd).unwrap();
        assert!(norm.ends_with("relative/path"));

        let entry_name = VolumeEntry {
            source: VolumeSource::Name("my_volume".to_string()),
            target: "/target".to_string(),
            readonly: false,
        };
        assert_eq!(entry_name.normalized_source(wd).unwrap(), "my_volume");
    }

    #[test]
    fn test_global_proxy_config_default() {
        let config = GlobalProxyConfig::default();
        assert_eq!(config.enabled, true);
        assert_eq!(config.strategy, ProxyStrategy::HttpsOnly);
        assert_eq!(config.trust_forwarded_headers, false);
    }

    #[test]
    fn test_proxy_strategy_default() {
        assert_eq!(ProxyStrategy::default(), ProxyStrategy::HttpsOnly);
    }

    #[test]
    fn test_restart_default() {
        assert_eq!(Restart::default(), Restart::No);
    }

    #[test]
    fn test_initialize_default() {
        assert_eq!(Initialize::default(), Initialize::Immediately);
    }

    #[test]
    fn test_pull_options_default() {
        assert_eq!(PullOptions::default(), PullOptions::OnStartup);
    }

    #[test]
    fn test_network_driver_default() {
        assert_eq!(NetworkDriver::default(), NetworkDriver::Bridge);
    }

    fn parse_telemetry_config(base_uri: &str) -> TelemetryConfig {
        let toml_str = format!(
            r#"
enabled = true
base_uri = "{base_uri}"
"#
        );
        toml::from_str(&toml_str).expect("Failed to parse TelemetryConfig")
    }

    #[test]
    fn test_telemetry_config_s3_uri() {
        let config = parse_telemetry_config("s3://my-bucket/dispenser");
        assert_eq!(
            config.table_uri_deployments().as_str(),
            "s3://my-bucket/dispenser/deployments"
        );
        assert_eq!(
            config.table_uri_status().as_str(),
            "s3://my-bucket/dispenser/status"
        );
        assert_eq!(
            config.table_uri_logs().as_str(),
            "s3://my-bucket/dispenser/logs"
        );
        assert_eq!(
            config.table_uri_traces().as_str(),
            "s3://my-bucket/dispenser/traces"
        );
        assert_eq!(
            config.table_uri_container_output().as_str(),
            "s3://my-bucket/dispenser/container-output"
        );
    }

    #[test]
    fn test_telemetry_config_gs_uri() {
        let config = parse_telemetry_config("gs://my-bucket/telemetry");
        assert_eq!(
            config.table_uri_deployments().as_str(),
            "gs://my-bucket/telemetry/deployments"
        );
    }

    #[test]
    fn test_telemetry_config_local_path() {
        let config = parse_telemetry_config("/home/user/data");
        assert_eq!(
            config.table_uri_deployments().as_str(),
            "file:///home/user/data/deployments"
        );
        assert_eq!(
            config.table_uri_status().as_str(),
            "file:///home/user/data/status"
        );
        assert_eq!(
            config.table_uri_container_output().as_str(),
            "file:///home/user/data/container-output"
        );
    }

    #[test]
    fn test_telemetry_config_file_uri() {
        let config = parse_telemetry_config("file:///var/log/dispenser");
        assert_eq!(
            config.table_uri_deployments().as_str(),
            "file:///var/log/dispenser/deployments"
        );
    }

    #[test]
    fn test_telemetry_config_trailing_slash() {
        let config = parse_telemetry_config("s3://my-bucket/dispenser/");
        assert_eq!(
            config.table_uri_deployments().as_str(),
            "s3://my-bucket/dispenser/deployments"
        );
    }

    #[test]
    fn test_telemetry_config_relative_dir_name() {
        let config = parse_telemetry_config("telem");
        let url = config.table_uri_deployments();
        // Should be resolved as file:// URL against CWD
        assert_eq!(url.scheme(), "file");
        assert!(
            url.as_str().ends_with("/telem/deployments"),
            "Expected URL ending with /telem/deployments, got: {}",
            url
        );
    }

    #[test]
    fn test_telemetry_config_relative_dot_slash() {
        let config = parse_telemetry_config("./data");
        let url = config.table_uri_deployments();
        assert_eq!(url.scheme(), "file");
        assert!(
            url.as_str().ends_with("/data/deployments"),
            "Expected URL ending with /data/deployments, got: {}",
            url
        );
    }

    #[test]
    fn test_extract_registry() {
        assert_eq!(extract_registry("ubuntu"), "docker.io");
        assert_eq!(extract_registry("ubuntu:latest"), "docker.io");
        assert_eq!(extract_registry("docker.io/library/ubuntu"), "docker.io");
        assert_eq!(extract_registry("ghcr.io/user/repo"), "ghcr.io");
        assert_eq!(
            extract_registry("localhost:5000/my-image"),
            "localhost:5000"
        );
        assert_eq!(
            extract_registry("myregistry.local:5000/image"),
            "myregistry.local:5000"
        );
        assert_eq!(extract_registry("quay.io/coreos/etcd"), "quay.io");
    }

    #[test]
    fn test_parse_image_reference() {
        // Tag references
        assert_eq!(
            parse_image_reference("ubuntu"),
            Image {
                registry: "docker.io".into(),
                name: "ubuntu".into(),
                tag: "latest".into()
            }
        );
        assert_eq!(
            parse_image_reference("ubuntu:20.04"),
            Image {
                registry: "docker.io".into(),
                name: "ubuntu".into(),
                tag: "20.04".into()
            }
        );
        assert_eq!(
            parse_image_reference("ghcr.io/user/repo:tag"),
            Image {
                registry: "ghcr.io".into(),
                name: "ghcr.io/user/repo".into(),
                tag: "tag".into()
            }
        );

        // Port numbers in registry
        assert_eq!(
            parse_image_reference("localhost:5000/my-image"),
            Image {
                registry: "localhost:5000".into(),
                name: "localhost:5000/my-image".into(),
                tag: "latest".into()
            }
        );
        assert_eq!(
            parse_image_reference("localhost:5000/my-image:1.0"),
            Image {
                registry: "localhost:5000".into(),
                name: "localhost:5000/my-image".into(),
                tag: "1.0".into()
            }
        );

        // Digest references
        assert_eq!(
            parse_image_reference(
                "ubuntu@sha256:45b23dee08af5e43a7fea6c4cf9c25ccf269ee113168c19722f87876677c5cb2"
            ),
            Image {
                registry: "docker.io".into(),
                name: "ubuntu".into(),
                tag: "@sha256:45b23dee08af5e43a7fea6c4cf9c25ccf269ee113168c19722f87876677c5cb2"
                    .into()
            }
        );
        assert_eq!(
            parse_image_reference("ghcr.io/user/repo@sha256:12345"),
            Image {
                registry: "ghcr.io".into(),
                name: "ghcr.io/user/repo".into(),
                tag: "@sha256:12345".into()
            }
        );
        assert_eq!(
            parse_image_reference("localhost:5000/image@sha256:123"),
            Image {
                registry: "localhost:5000".into(),
                name: "localhost:5000/image".into(),
                tag: "@sha256:123".into()
            }
        );

        assert_eq!(
            parse_image_reference("postgres:18"),
            Image {
                registry: "docker.io".into(),
                name: "postgres".into(),
                tag: "18".into()
            }
        );
    }
}
