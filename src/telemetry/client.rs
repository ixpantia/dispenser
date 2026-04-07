use super::events::{ContainerOutputEvent, ContainerStatusEvent, DeploymentEvent};
use super::types::{ContainerState, HealthStatus, TriggerType};
use crate::service::instance::ServiceInstance;
use crate::telemetry::service::TelemetryBuffers;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct TelemetryClient {
    pub buffers: Arc<TelemetryBuffers>,
    container_output_sequence: Arc<AtomicI64>,
}

impl TelemetryClient {
    pub fn new(buffers: Arc<TelemetryBuffers>) -> Self {
        Self {
            buffers,
            container_output_sequence: Arc::new(AtomicI64::new(0)),
        }
    }

    pub async fn track_deployment(
        &self,
        service: &ServiceInstance,
        container_id: &str,
        image_sha: &str,
        image_size_mb: i64,
        trigger_type: TriggerType,
        dispenser_version: &str,
        container_created_at: i64,
    ) {
        let now = chrono::Utc::now();
        let timestamp = now.timestamp_micros();

        let config = &service.config;
        let svc_entry = &config.service;

        let event = DeploymentEvent {
            event_id: Uuid::now_v7(),
            timestamp,
            service: &svc_entry.name,
            image: &svc_entry.image.name,
            image_sha,
            image_size_mb,
            container_id,
            container_created_at,
            trigger_type,
            dispenser_version,
            restart_policy: svc_entry.restart,
            memory_limit: svc_entry.memory.as_deref(),
            cpu_limit: svc_entry.cpus.as_deref(),
            proxy_enabled: config.proxy.is_some(),
            proxy_host: config.proxy.as_ref().map(|p| p.host.as_str()),
            port_mappings_count: config.ports.len() as i32,
            volume_count: config.volume.len() as i32,
            network_count: config.network.len() as i32,
        };

        self.buffers.push_deployments_event(event).await
    }

    pub async fn track_status(
        &self,
        service_name: &str,
        container_id: &str,
        state: ContainerState,
        health_status: HealthStatus,
        exit_code: Option<i32>,
        restart_count: i32,
        uptime_seconds: i64,
        failing_streak: i32,
        last_health_output: Option<&str>,
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

        self.buffers.push_status_event(event).await
    }

    pub async fn track_container_output(
        &self,
        service_name: &str,
        container_id: &str,
        stream: &str,
        message: &str,
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

        self.buffers.push_container_output(event).await
    }
}
