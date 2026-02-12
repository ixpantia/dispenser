use super::super::types::TriggerType;
use crate::service::file::Restart;
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
    pub trigger_type: TriggerType,
    pub dispenser_version: String,
    pub restart_policy: Restart,
    pub memory_limit: Option<String>,
    pub cpu_limit: Option<String>,
    pub proxy_enabled: bool,
    pub proxy_host: Option<String>,
    pub port_mappings_count: i32,
    pub volume_count: i32,
    pub network_count: i32,
}
