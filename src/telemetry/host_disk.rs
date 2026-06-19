use std::time::Duration;

use sysinfo::{Disks, System};

use crate::telemetry::events::{DispenserEvent, HostDiskEvent};

/// Spawns a background task that periodically samples disk metrics and sends them
/// through the telemetry channel. One event is sent per disk/mount point.
pub fn spawn_disk_monitor(tx: tokio::sync::mpsc::Sender<DispenserEvent>, interval_seconds: u64) {
    tokio::spawn(async move {
        let mut disks = Disks::new_with_refreshed_list();
        let mut interval = tokio::time::interval(Duration::from_secs(interval_seconds));
        interval.tick().await; // Skip the first immediate tick

        // Get hostname once at startup
        let hostname = System::host_name().unwrap_or_else(|| "unknown".to_string());

        loop {
            interval.tick().await;

            // Refresh disk data
            disks.refresh(true);

            // Get current timestamp in microseconds
            let timestamp = chrono::Utc::now().timestamp_micros();

            // Iterate over all disks and send an event for each
            for disk in disks.list() {
                let mount_point = disk.mount_point().to_string_lossy().to_string();
                let disk_name = disk.name().to_string_lossy().to_string();
                let file_system = disk.file_system().to_string_lossy().to_string();

                let total_space = disk.total_space();
                let available_space = disk.available_space();
                let used_space = total_space.saturating_sub(available_space);

                let usage_percent = if total_space > 0 {
                    (used_space as f64 / total_space as f64) * 100.0
                } else {
                    0.0
                };

                let event = HostDiskEvent {
                    hostname: hostname.clone(),
                    timestamp,
                    mount_point,
                    disk_name,
                    file_system,
                    total_space,
                    used_space,
                    available_space,
                    usage_percent,
                };

                if tx
                    .send(DispenserEvent::HostDisk(Box::new(event)))
                    .await
                    .is_err()
                {
                    log::debug!("Disk monitor: telemetry channel closed, stopping");
                    break;
                }
            }
        }
    });
}