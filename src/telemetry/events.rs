use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentEvent {
    pub event_id: Uuid,
    /// Timestamp in microseconds (UTC)
    pub timestamp: i64,
    pub service: String,
    pub image: String,
    pub image_sha: String,
    pub image_size_mb: i64,
    pub container_id: String,
    /// Timestamp in microseconds (UTC)
    pub container_created_at: i64,
    pub trigger_type: String,
    pub dispenser_version: String,
    pub restart_policy: String,
    pub memory_limit: Option<String>,
    pub cpu_limit: Option<String>,
    pub proxy_enabled: bool,
    pub proxy_host: Option<String>,
    pub port_mappings_count: i32,
    pub volume_count: i32,
    pub network_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerStatusEvent {
    pub event_id: Uuid,
    /// Timestamp in microseconds (UTC)
    pub timestamp: i64,
    pub service: String,
    pub container_id: String,
    pub state: String,
    pub health_status: String,
    pub exit_code: Option<i32>,
    pub restart_count: i32,
    pub uptime_seconds: i64,
    pub failing_streak: i32,
    pub last_health_output: Option<String>,
}

#[derive(Debug)]
pub enum DispenserEvent {
    Deployment(Box<DeploymentEvent>),
    ContainerStatus(Box<ContainerStatusEvent>),
}

