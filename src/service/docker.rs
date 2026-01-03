//! Docker client module using bollard.
//!
//! This module provides a shared Docker client instance and helper functions
//! for interacting with Docker via the bollard API.

use bollard::Docker;
use std::sync::OnceLock;

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
