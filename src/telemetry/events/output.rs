use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerOutputEvent<'a> {
    /// Timestamp in microseconds (UTC)
    pub timestamp: i64,
    pub service: &'a str,
    pub container_id: &'a str,
    pub stream: &'a str,
    pub message: &'a str,
    pub sequence: i64,
}
