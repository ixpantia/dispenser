use super::super::types::TriggerType;
use crate::service::file::Restart;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentEvent<'a> {
    pub event_id: Uuid,
    /// Timestamp in microseconds (UTC)
    pub timestamp: i64,
    pub service: &'a str,
    pub image: &'a str,
    pub image_sha: &'a str,
    pub image_size_mb: i64,
    pub container_id: &'a str,
    /// Timestamp in microseconds (UTC)
    pub container_created_at: i64,
    pub trigger_type: TriggerType,
    pub dispenser_version: &'a str,
    pub restart_policy: Restart,
    pub memory_limit: Option<&'a str>,
    pub cpu_limit: Option<&'a str>,
    pub proxy_enabled: bool,
    pub proxy_host: Option<&'a str>,
    pub port_mappings_count: i32,
    pub volume_count: i32,
    pub network_count: i32,
}
