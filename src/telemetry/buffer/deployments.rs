use super::super::events::DeploymentEvent;
use super::super::schema::deployments_schema;
use arrow::array::{
    BooleanBuilder, Date32Builder, Int32Builder, Int64Builder, StringBuilder,
    StringDictionaryBuilder, TimestampMicrosecondBuilder,
};
use arrow::datatypes::Int8Type;
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
    trigger_type: StringDictionaryBuilder<Int8Type>,
    dispenser_version: StringBuilder,
    restart_policy: StringDictionaryBuilder<Int8Type>,
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
            trigger_type: StringDictionaryBuilder::new(),
            dispenser_version: StringBuilder::with_capacity(capacity, capacity * 10),
            restart_policy: StringDictionaryBuilder::new(),
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
        self.trigger_type.append_value(event.trigger_type.as_ref());
        self.dispenser_version
            .append_value(&event.dispenser_version);

        let restart_policy_str = match event.restart_policy {
            crate::service::file::Restart::Always => "always",
            crate::service::file::Restart::No => "no",
            crate::service::file::Restart::OnFailure => "on-failure",
            crate::service::file::Restart::UnlessStopped => "unless-stopped",
        };
        self.restart_policy.append_value(restart_policy_str);

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
