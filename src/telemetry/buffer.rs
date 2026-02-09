use super::events::{ContainerStatusEvent, DeploymentEvent};
use super::schema::{deployments_schema, status_schema};
use arrow::array::{
    BooleanBuilder, Date32Builder, Int32Builder, Int64Builder, StringBuilder,
    TimestampMicrosecondBuilder,
};
use arrow::record_batch::RecordBatch;
use std::sync::Arc;

pub struct DeploymentsBuffer {
    date: Date32Builder,
    timestamp: TimestampMicrosecondBuilder,
    service: StringBuilder,
    image: StringBuilder,
    image_sha: StringBuilder,
    image_size_mb: Int64Builder,
    container_id: StringBuilder,
    container_created_at: TimestampMicrosecondBuilder,
    trigger_type: StringBuilder,
    dispenser_version: StringBuilder,
    restart_policy: StringBuilder,
    memory_limit: StringBuilder,
    cpu_limit: StringBuilder,
    proxy_enabled: BooleanBuilder,
    proxy_host: StringBuilder,
    port_mappings_count: Int32Builder,
    volume_count: Int32Builder,
    network_count: Int32Builder,

    count: usize,
}

impl DeploymentsBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            date: Date32Builder::with_capacity(capacity),
            timestamp: TimestampMicrosecondBuilder::with_capacity(capacity),
            service: StringBuilder::with_capacity(capacity, capacity * 20),
            image: StringBuilder::with_capacity(capacity, capacity * 50),
            image_sha: StringBuilder::with_capacity(capacity, capacity * 64),
            image_size_mb: Int64Builder::with_capacity(capacity),
            container_id: StringBuilder::with_capacity(capacity, capacity * 64),
            container_created_at: TimestampMicrosecondBuilder::with_capacity(capacity),
            trigger_type: StringBuilder::with_capacity(capacity, capacity * 10),
            dispenser_version: StringBuilder::with_capacity(capacity, capacity * 10),
            restart_policy: StringBuilder::with_capacity(capacity, capacity * 10),
            memory_limit: StringBuilder::with_capacity(capacity, capacity * 5),
            cpu_limit: StringBuilder::with_capacity(capacity, capacity * 5),
            proxy_enabled: BooleanBuilder::with_capacity(capacity),
            proxy_host: StringBuilder::with_capacity(capacity, capacity * 30),
            port_mappings_count: Int32Builder::with_capacity(capacity),
            volume_count: Int32Builder::with_capacity(capacity),
            network_count: Int32Builder::with_capacity(capacity),
            count: 0,
        }
    }

    pub fn push(&mut self, event: &DeploymentEvent) {
        let date_days = (event.timestamp / (86400 * 1_000_000)) as i32;

        self.date.append_value(date_days);
        self.timestamp.append_value(event.timestamp);
        self.service.append_value(&event.service);
        self.image.append_value(&event.image);
        self.image_sha.append_value(&event.image_sha);
        self.image_size_mb.append_value(event.image_size_mb);
        self.container_id.append_value(&event.container_id);
        self.container_created_at
            .append_value(event.container_created_at);
        self.trigger_type.append_value(&event.trigger_type);
        self.dispenser_version
            .append_value(&event.dispenser_version);
        self.restart_policy.append_value(&event.restart_policy);

        if let Some(val) = &event.memory_limit {
            self.memory_limit.append_value(val);
        } else {
            self.memory_limit.append_null();
        }

        if let Some(val) = &event.cpu_limit {
            self.cpu_limit.append_value(val);
        } else {
            self.cpu_limit.append_null();
        }

        self.proxy_enabled.append_value(event.proxy_enabled);

        if let Some(val) = &event.proxy_host {
            self.proxy_host.append_value(val);
        } else {
            self.proxy_host.append_null();
        }

        self.port_mappings_count
            .append_value(event.port_mappings_count);
        self.volume_count.append_value(event.volume_count);
        self.network_count.append_value(event.network_count);

        self.count += 1;
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn into_record_batch(mut self) -> arrow::error::Result<RecordBatch> {
        let schema = deployments_schema();

        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(self.date.finish()),
                Arc::new(self.timestamp.finish().with_timezone("UTC")),
                Arc::new(self.service.finish()),
                Arc::new(self.image.finish()),
                Arc::new(self.image_sha.finish()),
                Arc::new(self.image_size_mb.finish()),
                Arc::new(self.container_id.finish()),
                Arc::new(self.container_created_at.finish().with_timezone("UTC")),
                Arc::new(self.trigger_type.finish()),
                Arc::new(self.dispenser_version.finish()),
                Arc::new(self.restart_policy.finish()),
                Arc::new(self.memory_limit.finish()),
                Arc::new(self.cpu_limit.finish()),
                Arc::new(self.proxy_enabled.finish()),
                Arc::new(self.proxy_host.finish()),
                Arc::new(self.port_mappings_count.finish()),
                Arc::new(self.volume_count.finish()),
                Arc::new(self.network_count.finish()),
            ],
        )
    }
}

pub struct StatusBuffer {
    date: Date32Builder,
    timestamp: TimestampMicrosecondBuilder,
    service: StringBuilder,
    container_id: StringBuilder,
    state: StringBuilder,
    health_status: StringBuilder,
    exit_code: Int32Builder,
    restart_count: Int32Builder,
    uptime_seconds: Int64Builder,
    failing_streak: Int32Builder,
    last_health_output: StringBuilder,

    count: usize,
}

impl StatusBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            date: Date32Builder::with_capacity(capacity),
            timestamp: TimestampMicrosecondBuilder::with_capacity(capacity),
            service: StringBuilder::with_capacity(capacity, capacity * 20),
            container_id: StringBuilder::with_capacity(capacity, capacity * 64),
            state: StringBuilder::with_capacity(capacity, capacity * 10),
            health_status: StringBuilder::with_capacity(capacity, capacity * 10),
            exit_code: Int32Builder::with_capacity(capacity),
            restart_count: Int32Builder::with_capacity(capacity),
            uptime_seconds: Int64Builder::with_capacity(capacity),
            failing_streak: Int32Builder::with_capacity(capacity),
            last_health_output: StringBuilder::with_capacity(capacity, capacity * 50),
            count: 0,
        }
    }

    pub fn push(&mut self, event: &ContainerStatusEvent) {
        let date_days = (event.timestamp / (86400 * 1_000_000)) as i32;

        self.date.append_value(date_days);
        self.timestamp.append_value(event.timestamp);
        self.service.append_value(&event.service);
        self.container_id.append_value(&event.container_id);
        self.state.append_value(&event.state);
        self.health_status.append_value(&event.health_status);

        if let Some(val) = event.exit_code {
            self.exit_code.append_value(val);
        } else {
            self.exit_code.append_null();
        }

        self.restart_count.append_value(event.restart_count);
        self.uptime_seconds.append_value(event.uptime_seconds);
        self.failing_streak.append_value(event.failing_streak);

        if let Some(val) = &event.last_health_output {
            self.last_health_output.append_value(val);
        } else {
            self.last_health_output.append_null();
        }

        self.count += 1;
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn into_record_batch(mut self) -> arrow::error::Result<RecordBatch> {
        let schema = status_schema();

        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(self.date.finish()),
                Arc::new(self.timestamp.finish().with_timezone("UTC")),
                Arc::new(self.service.finish()),
                Arc::new(self.container_id.finish()),
                Arc::new(self.state.finish()),
                Arc::new(self.health_status.finish()),
                Arc::new(self.exit_code.finish()),
                Arc::new(self.restart_count.finish()),
                Arc::new(self.uptime_seconds.finish()),
                Arc::new(self.failing_streak.finish()),
                Arc::new(self.last_health_output.finish()),
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::events::DeploymentEvent;
    use arrow::datatypes::{DataType, TimeUnit};
    use uuid::Uuid;

    #[test]
    fn test_deployments_buffer_schema_compliance() {
        let mut buffer = DeploymentsBuffer::new(10);

        let event = DeploymentEvent {
            event_id: Uuid::now_v7(),
            timestamp: 1700000000000000, // UTC mic
            service: "test-svc".to_string(),
            image: "nginx".to_string(),
            image_sha: "sha256:1234".to_string(),
            image_size_mb: 100,
            container_id: "cid-1".to_string(),
            container_created_at: 1700000000000000,
            trigger_type: "manual".to_string(),
            dispenser_version: "1.0".to_string(),
            restart_policy: "always".to_string(),
            memory_limit: Some("512m".to_string()),
            cpu_limit: None,
            proxy_enabled: true,
            proxy_host: Some("test.com".to_string()),
            port_mappings_count: 2,
            volume_count: 1,
            network_count: 1,
        };

        buffer.push(&event);
        assert!(!buffer.is_empty());
        assert_eq!(buffer.len(), 1);

        let batch = buffer
            .into_record_batch()
            .expect("Failed to create record batch");

        // Verify Schema correctness
        let schema = batch.schema();

        // Check Timestamp is UTC (Crucial fix verification)
        let ts_field = schema.field_with_name("timestamp").unwrap();
        match ts_field.data_type() {
            DataType::Timestamp(TimeUnit::Microsecond, Some(tz)) => {
                assert_eq!(tz.as_ref(), "UTC", "Timestamp timezone mismatch");
            }
            dt => panic!("Expected Timestamp(Microsecond, UTC), found {:?}", dt),
        }

        // Check Nullable fields
        let cpu_field = schema.field_with_name("cpu_limit").unwrap();
        assert!(cpu_field.is_nullable());
    }
}
