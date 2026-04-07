//! Docker client module using bollard.
//!
//! This module provides a shared Docker client instance and helper functions
//! for interacting with Docker via the bollard API, including asynchronous
//! credential support from the Docker CLI configuration.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use bollard::auth::DockerCredentials;
use bollard::Docker;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
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

/// Get Docker credentials for multiple registries.
/// This avoids duplicate calls to credential helpers when multiple services use the same registry.
pub async fn get_credentials_for_registries(
    registries: &HashSet<Box<str>>,
) -> HashMap<Box<str>, DockerCredentials> {
    let mut credentials_map = HashMap::new();
    for registry in registries {
        if let Some(creds) = get_credentials(registry).await {
            credentials_map.insert(registry.clone(), creds);
        }
    }
    credentials_map
}

/// Get Docker credentials for a given registry from the Docker config file (~/.docker/config.json).
/// This supports static auth strings, the global 'credsStore', and per-registry 'credHelpers'.
pub async fn get_credentials(registry: &str) -> Option<DockerCredentials> {
    let config_path = match std::env::var("DOCKER_CONFIG")
        .map(|p| std::path::PathBuf::from(p).join("config.json"))
        .or_else(|_| {
            std::env::var("HOME").map(|h| std::path::PathBuf::from(h).join(".docker/config.json"))
        }) {
        Ok(path) => path,
        Err(e) => {
            log::debug!("Could not determine Docker config path: {}", e);
            return None;
        }
    };

    let content = match tokio::fs::read_to_string(&config_path).await {
        Ok(content) => content,
        Err(e) => {
            log::debug!(
                "Could not read Docker config file at {:?}: {}",
                config_path,
                e
            );
            return None;
        }
    };

    let config: DockerConfig = match serde_json::from_str(&content) {
        Ok(config) => config,
        Err(e) => {
            log::error!("Failed to parse Docker config at {:?}: {}", config_path, e);
            return None;
        }
    };

    // 1. Try Credential Helpers (Specific helper for registry)
    if let Some(helpers) = &config.cred_helpers {
        if let Some(helper_suffix) = helpers.get(registry) {
            log::debug!(
                "Found specific credHelper '{}' for registry '{}'",
                helper_suffix,
                registry
            );
            if let Some(creds) = call_credential_helper(helper_suffix, registry).await {
                return Some(creds);
            }
            log::warn!(
                "Credential helper '{}' failed to provide credentials for '{}'",
                helper_suffix,
                registry
            );
        }
    }

    // 2. Try Static Auths in config.json
    if let Some(auths) = &config.auths {
        let keys_to_check = get_registry_keys(registry);
        for key in keys_to_check {
            if let Some(auth_entry) = auths.get(&key) {
                if let Some(auth) = &auth_entry.auth {
                    log::debug!("Found static auth entry for registry key '{}'", key);
                    // The 'auth' field in config.json is base64(username:password)
                    if let Ok(decoded_bytes) = STANDARD.decode(auth) {
                        if let Ok(decoded) = String::from_utf8(decoded_bytes) {
                            let mut parts = decoded.splitn(2, ':');
                            if let (Some(username), Some(password)) = (parts.next(), parts.next()) {
                                return Some(DockerCredentials {
                                    username: Some(username.to_string()),
                                    password: Some(password.to_string()),
                                    ..Default::default()
                                });
                            }
                        }
                    }

                    // Fallback to passing the raw auth string if decoding/parsing fails
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
        log::debug!(
            "Found global credsStore '{}' for registry '{}'",
            helper_suffix,
            registry
        );
        if let Some(creds) = call_credential_helper(helper_suffix, registry).await {
            return Some(creds);
        }
        log::warn!(
            "Global credsStore '{}' failed to provide credentials for '{}'",
            helper_suffix,
            registry
        );
    }

    log::debug!("No Docker credentials found for registry '{}'", registry);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_registry_keys() {
        // Standard registry
        let keys = get_registry_keys("ghcr.io");
        assert_eq!(keys, vec!["ghcr.io", "https://ghcr.io"]);

        // Http registry
        let keys = get_registry_keys("http://localhost:5000");
        assert_eq!(keys, vec!["http://localhost:5000"]);

        // Docker hub
        let keys = get_registry_keys("docker.io");
        assert!(keys.contains(&"docker.io".to_string()));
        assert!(keys.contains(&"https://docker.io".to_string()));
        assert!(keys.contains(&"https://index.docker.io/v1/".to_string()));
        assert!(keys.contains(&"index.docker.io/v1/".to_string()));
        assert!(keys.contains(&"https://registry-1.docker.io/v2/".to_string()));
    }

    #[test]
    fn test_docker_config_parsing_static_auth() {
        // base64("user:pass") = "dXNlcjpwYXNz"
        let config_json = r#"{
            "auths": {
                "ghcr.io": {
                    "auth": "dXNlcjpwYXNz"
                }
            }
        }"#;

        let config: DockerConfig = serde_json::from_str(config_json).unwrap();
        let auths = config.auths.as_ref().unwrap();
        let auth = auths.get("ghcr.io").unwrap().auth.as_ref().unwrap();

        // Simulate the logic in get_credentials
        let decoded_bytes = STANDARD.decode(auth).unwrap();
        let decoded = String::from_utf8(decoded_bytes).unwrap();
        let mut parts = decoded.splitn(2, ':');
        let username = parts.next().unwrap();
        let password = parts.next().unwrap();

        assert_eq!(username, "user");
        assert_eq!(password, "pass");
    }

    #[test]
    fn test_docker_config_parsing_password_with_colon() {
        // base64("robot:p:a:s:s") = "cm9ib3Q6cDphOnM6cw=="
        let config_json = r#"{
            "auths": {
                "myregistry.local": {
                    "auth": "cm9ib3Q6cDphOnM6cw=="
                }
            }
        }"#;

        let config: DockerConfig = serde_json::from_str(config_json).unwrap();
        let auths = config.auths.as_ref().unwrap();
        let auth = auths
            .get("myregistry.local")
            .unwrap()
            .auth
            .as_ref()
            .unwrap();

        // Simulate the logic in get_credentials
        let decoded_bytes = STANDARD.decode(auth).unwrap();
        let decoded = String::from_utf8(decoded_bytes).unwrap();
        let mut parts = decoded.splitn(2, ':');
        let username = parts.next().unwrap();
        let password = parts.next().unwrap();

        assert_eq!(username, "robot");
        assert_eq!(password, "p:a:s:s");
    }
}
