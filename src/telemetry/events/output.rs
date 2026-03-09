use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerOutputEvent {
    /// Timestamp in microseconds (UTC)
    pub timestamp: i64,
    pub service: String,
    pub container_id: String,
    pub stream: String,
    pub message: String,
    pub sequence: i64,
}
