use serde::{Deserialize, Serialize};

/// Memory usage event for the host machine running dispenser.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostMemoryEvent {
    /// The Hostname of the machine being tracked
    pub hostname: String,
    /// Unix timestamp in microseconds
    pub timestamp: i64,
    /// Total RAM in bytes
    pub total_memory: u64,
    /// Used RAM in bytes
    pub used_memory: u64,
    /// Available RAM in bytes
    pub available_memory: u64,
    /// Memory usage percentage (0-100)
    pub memory_usage_percent: f64,
    /// Total swap space in bytes
    pub total_swap: u64,
    /// Used swap space in bytes
    pub used_swap: u64,
    /// Swap usage percentage (0-100)
    pub swap_usage_percent: f64,
}
