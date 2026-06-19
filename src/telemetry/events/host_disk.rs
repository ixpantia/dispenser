use serde::{Deserialize, Serialize};

/// Disk usage event for the host machine running dispenser.
/// One event per disk/mount point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostDiskEvent {
    /// The Hostname of the machine being tracked
    pub hostname: String,
    /// Unix timestamp in microseconds
    pub timestamp: i64,
    /// Mount point path (e.g., "/", "/home")
    pub mount_point: String,
    /// Disk/device name (e.g., "sda1", "nvme0n1p2")
    pub disk_name: String,
    /// File system type (e.g., "ext4", "xfs", "ntfs")
    pub file_system: String,
    /// Total disk space in bytes
    pub total_space: u64,
    /// Used disk space in bytes
    pub used_space: u64,
    /// Available disk space in bytes
    pub available_space: u64,
    /// Disk usage percentage (0-100)
    pub usage_percent: f64,
}