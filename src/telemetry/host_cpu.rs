use std::time::Duration;

use sysinfo::System;

use crate::telemetry::events::{DispenserEvent, HostCpuEvent};

/// Spawns a background task that periodically samples CPU metrics and sends them
/// through the telemetry channel.
pub fn spawn_cpu_monitor(tx: tokio::sync::mpsc::Sender<DispenserEvent>, interval_seconds: u64) {
    tokio::spawn(async move {
        let mut sys = System::new_all();
        let mut interval = tokio::time::interval(Duration::from_secs(interval_seconds));
        interval.tick().await; // Skip the first immediate tick

        // Get hostname once at startup
        let hostname = sysinfo::System::host_name().unwrap_or_else(|| "unknown".to_string());

        // According to the sysinfo docs:
        // Please note that the result will be inaccurate at the first call.
        // You need to call this method at least twice (with a bit of time
        // between each call, like 200 ms, take a look at
        // MINIMUM_CPU_UPDATE_INTERVAL for more information) to get accurate
        // value as it uses previous results to compute the next value.
        sys.refresh_cpu_all();

        loop {
            interval.tick().await;

            // Refresh CPU data
            sys.refresh_cpu_all();

            // Get CPU utilization (global)
            let cpus = sys.cpus();
            if cpus.is_empty() {
                continue;
            }

            // Get global CPU usage percentage
            let core_count = cpus.len() as u32;

            // Get load averages (1, 5, 15 minutes)
            let load_avg = System::load_average();
            let load_avg_1m = load_avg.one;
            let load_avg_5m = load_avg.five;
            let load_avg_15m = load_avg.fifteen;

            // Get current timestamp in microseconds
            let timestamp = chrono::Utc::now().timestamp_micros();

            let event = HostCpuEvent {
                hostname: hostname.clone(),
                timestamp,
                load_avg_1m: load_avg_1m as f64,
                load_avg_5m: load_avg_5m as f64,
                load_avg_15m: load_avg_15m as f64,
                core_count,
            };

            if tx.send(DispenserEvent::HostCpu(Box::new(event))).await.is_err() {
                log::debug!("CPU monitor: telemetry channel closed, stopping");
                break;
            }
        }
    });
}
