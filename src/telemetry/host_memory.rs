use std::time::Duration;

use sysinfo::System;

use crate::telemetry::events::{DispenserEvent, HostMemoryEvent};

/// Spawns a background task that periodically samples memory metrics and sends them
/// through the telemetry channel.
pub fn spawn_memory_monitor(tx: tokio::sync::mpsc::Sender<DispenserEvent>, interval_seconds: u64) {
    tokio::spawn(async move {
        let mut sys = System::new_all();
        let mut interval = tokio::time::interval(Duration::from_secs(interval_seconds));
        interval.tick().await; // Skip the first immediate tick

        // Get hostname once at startup
        let hostname = sysinfo::System::host_name().unwrap_or_else(|| "unknown".to_string());

        loop {
            interval.tick().await;

            // Refresh memory data
            sys.refresh_memory();

            // Get memory metrics
            let total_memory = sys.total_memory();
            let used_memory = sys.used_memory();
            let available_memory = sys.available_memory();

            // Calculate memory usage percentage
            let memory_usage_percent = if total_memory > 0 {
                (used_memory as f64 / total_memory as f64) * 100.0
            } else {
                0.0
            };

            // Get swap metrics
            let total_swap = sys.total_swap();
            let used_swap = sys.used_swap();

            // Calculate swap usage percentage
            let swap_usage_percent = if total_swap > 0 {
                (used_swap as f64 / total_swap as f64) * 100.0
            } else {
                0.0
            };

            // Get current timestamp in microseconds
            let timestamp = chrono::Utc::now().timestamp_micros();

            let event = HostMemoryEvent {
                hostname: hostname.clone(),
                timestamp,
                total_memory,
                used_memory,
                available_memory,
                memory_usage_percent,
                total_swap,
                used_swap,
                swap_usage_percent,
            };

            if tx
                .send(DispenserEvent::HostMemory(Box::new(event)))
                .await
                .is_err()
            {
                log::debug!("Memory monitor: telemetry channel closed, stopping");
                break;
            }
        }
    });
}
