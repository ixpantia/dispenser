use super::events::{ContainerOutputEvent, ContainerStatusEvent, DeploymentEvent, DispenserEvent};
use super::types::{ContainerState, HealthStatus, TriggerType};
use crate::service::instance::ServiceInstance;
use log::error;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct TelemetryClient {
    tx: Sender<DispenserEvent>,
    container_output_sequence: Arc<AtomicI64>,
}

impl TelemetryClient {
    pub fn new(tx: Sender<DispenserEvent>) -> Self {
        Self {
            tx,
            container_output_sequence: Arc::new(AtomicI64::new(0)),
        }
    }

    pub fn track_deployment(
        &self,
        service: &ServiceInstance,
        container_id: String,
        image_sha: String,
        image_size_mb: i64,
        trigger_type: TriggerType,
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
            restart_policy: svc_entry.restart.clone(),
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
        state: ContainerState,
        health_status: HealthStatus,
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

    pub fn track_container_output(
        &self,
        service_name: String,
        container_id: String,
        stream: String,
        message: String,
    ) {
        let now = chrono::Utc::now();
        let timestamp = now.timestamp_micros();

        let sequence = self
            .container_output_sequence
            .fetch_add(1, Ordering::SeqCst);

        let event = ContainerOutputEvent {
            timestamp,
            service: service_name,
            container_id,
            stream,
            message,
            sequence,
        };

        self.send(DispenserEvent::ContainerOutput(event));
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
