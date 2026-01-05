//! Docker client module using bollard.
//!
//! This module provides a shared Docker client instance and helper functions
//! for interacting with Docker via the bollard API, including asynchronous
//! credential support from the Docker CLI configuration.

use bollard::auth::DockerCredentials;
use bollard::Docker;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

static DOCKER_CLIENT: OnceLock<Docker> = OnceLock::new();

/// Get a reference to the shared Docker client.
///
/// This lazily initializes the Docker client on first use.
/// The client connects to Docker using the default connection method
/// (Unix socket on Linux/macOS, named pipe on Windows).
pub fn get_docker() -> &'static Docker {
    DOCKER_CLIENT.get_or_init(|| {
        Docker::connect_with_local_defaults().expect("Failed to connect to Docker daemon")
    })
}

#[derive(Deserialize, Debug)]
struct DockerConfig {
    auths: Option<HashMap<String, DockerConfigAuth>>,
    #[serde(rename = "credsStore")]
    creds_store: Option<String>,
    #[serde(rename = "credHelpers")]
    cred_helpers: Option<HashMap<String, String>>,
}

#[derive(Deserialize, Debug)]
struct DockerConfigAuth {
    auth: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct CredentialHelperResponse {
    username: Option<String>,
    secret: Option<String>,
}

/// Get Docker credentials for a given registry from the Docker config file (~/.docker/config.json).
/// This supports static auth strings, the global 'credsStore', and per-registry 'credHelpers'.
pub async fn get_credentials(registry: &str) -> Option<DockerCredentials> {
    let config_path = std::env::var("DOCKER_CONFIG")
        .map(|p| std::path::PathBuf::from(p).join("config.json"))
        .or_else(|_| {
            std::env::var("HOME").map(|h| std::path::PathBuf::from(h).join(".docker/config.json"))
        })
        .ok()?;

    let content = tokio::fs::read_to_string(config_path).await.ok()?;
    let config: DockerConfig = serde_json::from_str(&content).ok()?;

    // 1. Try Credential Helpers (Specific helper for registry)
    if let Some(helpers) = &config.cred_helpers {
        if let Some(helper_suffix) = helpers.get(registry) {
            if let Some(creds) = call_credential_helper(helper_suffix, registry).await {
                return Some(creds);
            }
        }
    }

    // 2. Try Static Auths in config.json
    if let Some(auths) = &config.auths {
        let keys_to_check = get_registry_keys(registry);
        for key in keys_to_check {
            if let Some(auth_entry) = auths.get(&key) {
                if let Some(auth) = &auth_entry.auth {
                    return Some(DockerCredentials {
                        auth: Some(auth.clone()),
                        ..Default::default()
                    });
                }
            }
        }
    }

    // 3. Try Global Credentials Store
    if let Some(helper_suffix) = &config.creds_store {
        if let Some(creds) = call_credential_helper(helper_suffix, registry).await {
            return Some(creds);
        }
    }

    None
}

/// Calls a docker-credential-helper (like 'osxkeychain', 'secretservice', 'wincred')
async fn call_credential_helper(helper_suffix: &str, registry: &str) -> Option<DockerCredentials> {
    let helper_cmd = format!("docker-credential-{}", helper_suffix);
    let mut child = Command::new(helper_cmd)
        .arg("get")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .ok()?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(registry.as_bytes()).await;
        let _ = stdin.flush().await;
    }

    let output = child.wait_with_output().await.ok()?;

    if output.status.success() {
        let creds: CredentialHelperResponse = serde_json::from_slice(&output.stdout).ok()?;
        if let (Some(username), Some(password)) = (creds.username, creds.secret) {
            return Some(DockerCredentials {
                username: Some(username),
                password: Some(password),
                ..Default::default()
            });
        }
    }
    None
}

/// Generates a list of possible keys in config.json for a given registry
fn get_registry_keys(registry: &str) -> Vec<String> {
    let mut keys = vec![registry.to_string()];

    if !registry.starts_with("http") {
        keys.push(format!("https://{}", registry));
    }

    if registry == "docker.io"
        || registry == "registry-1.docker.io"
        || registry == "index.docker.io"
    {
        keys.push("https://index.docker.io/v1/".to_string());
        keys.push("index.docker.io/v1/".to_string());
        keys.push("https://registry-1.docker.io/v2/".to_string());
    }

    keys
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
pub fn parse_image_reference(image: &str) -> (&str, &str) {
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
