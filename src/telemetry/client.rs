use super::events::{ContainerStatusEvent, DeploymentEvent, DispenserEvent};
use crate::service::instance::ServiceInstance;
use log::error;
use tokio::sync::mpsc::Sender;
use uuid::Uuid;

#[derive(Clone)]
pub struct TelemetryClient {
    tx: Sender<DispenserEvent>,
}

impl TelemetryClient {
    pub fn new(tx: Sender<DispenserEvent>) -> Self {
        Self { tx }
    }

    pub fn track_deployment(
        &self,
        service: &ServiceInstance,
        container_id: String,
        image_sha: String,
        image_size_mb: i64,
        trigger_type: String,
        dispenser_version: String,
        container_created_at: i64,
    ) {
        let now = chrono::Utc::now();
        let timestamp = now.timestamp_micros();

        let config = &service.config;
        let svc_entry = &config.service;

        let event = DeploymentEvent {
            event_id: Uuid::now_v7(),
            timestamp,
            service: svc_entry.name.clone(),
            image: svc_entry.image.clone(),
            image_sha,
            image_size_mb,
            container_id,
            container_created_at,
            trigger_type,
            dispenser_version,
            restart_policy: format!("{:?}", svc_entry.restart),
            memory_limit: svc_entry.memory.clone(),
            cpu_limit: svc_entry.cpus.clone(),
            proxy_enabled: config.proxy.is_some(),
            proxy_host: config.proxy.as_ref().map(|p| p.host.clone()),
            port_mappings_count: config.ports.len() as i32,
            volume_count: config.volume.len() as i32,
            network_count: config.network.len() as i32,
        };

        self.send(DispenserEvent::Deployment(Box::new(event)));
    }

    pub fn track_status(
        &self,
        service_name: String,
        container_id: String,
        state: String,
        health_status: String,
        exit_code: Option<i32>,
        restart_count: i32,
        uptime_seconds: i64,
        failing_streak: i32,
        last_health_output: Option<String>,
    ) {
        let now = chrono::Utc::now();
        let timestamp = now.timestamp_micros();

        let event = ContainerStatusEvent {
            event_id: Uuid::now_v7(),
            timestamp,
            service: service_name,
            container_id,
            state,
            health_status,
            exit_code,
            restart_count,
            uptime_seconds,
            failing_streak,
            last_health_output,
        };

        self.send(DispenserEvent::ContainerStatus(Box::new(event)));
    }

    fn send(&self, event: DispenserEvent) {
        // Use try_send to avoid blocking the main loop.
        // If the channel is full, we drop the event and log an error.
        if let Err(e) = self.tx.try_send(event) {
            error!(
                "Failed to send telemetry event (channel full or closed): {:?}",
                e
            );
        }
    }
}
