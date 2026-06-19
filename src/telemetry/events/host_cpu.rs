use serde::{Deserialize, Serialize};

/// CPU usage event for the host machine running dispenser.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostCpuEvent {
    /// The Hostname of the machine being tracked
    pub hostname: String,
    /// Unix timestamp in microseconds
    pub timestamp: i64,
    /// 1-minute load average
    pub load_avg_1m: f64,
    /// 5-minute load average
    pub load_avg_5m: f64,
    /// 15-minute load average
    pub load_avg_15m: f64,
    /// Number of CPU cores
    pub core_count: u32,
}