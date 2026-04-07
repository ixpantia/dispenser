use super::super::types::{ContainerState, HealthStatus};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerStatusEvent<'a> {
    pub event_id: Uuid,
    /// Timestamp in microseconds (UTC)
    pub timestamp: i64,
    pub service: &'a str,
    pub container_id: &'a str,
    pub state: ContainerState,
    pub health_status: HealthStatus,
    pub exit_code: Option<i32>,
    pub restart_count: i32,
    pub uptime_seconds: i64,
    pub failing_streak: i32,
    pub last_health_output: Option<&'a str>,
}
