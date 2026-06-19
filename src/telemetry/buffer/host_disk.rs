use super::super::events::HostDiskEvent;
use super::super::schema::host_disk_schema;
use arrow::array::{
    Date32Builder, Float64Builder, Int64Builder, StringBuilder, TimestampMicrosecondBuilder,
};
use arrow::record_batch::RecordBatch;
use std::sync::Arc;

pub struct HostDiskBuffer {
    date: Date32Builder,
    timestamp: TimestampMicrosecondBuilder,
    hostname: StringBuilder,
    mount_point: StringBuilder,
    disk_name: StringBuilder,
    file_system: StringBuilder,
    total_space: Int64Builder,
    used_space: Int64Builder,
    available_space: Int64Builder,
    usage_percent: Float64Builder,

    count: usize,
}

impl HostDiskBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            date: Date32Builder::with_capacity(capacity),
            timestamp: TimestampMicrosecondBuilder::with_capacity(capacity),
            hostname: StringBuilder::with_capacity(capacity, capacity * 64),
            mount_point: StringBuilder::with_capacity(capacity, capacity * 256),
            disk_name: StringBuilder::with_capacity(capacity, capacity * 64),
            file_system: StringBuilder::with_capacity(capacity, capacity * 32),
            total_space: Int64Builder::with_capacity(capacity),
            used_space: Int64Builder::with_capacity(capacity),
            available_space: Int64Builder::with_capacity(capacity),
            usage_percent: Float64Builder::with_capacity(capacity),
            count: 0,
        }
    }

    pub fn push(&mut self, event: &HostDiskEvent) {
        let date_days = (event.timestamp / (86400 * 1_000_000)) as i32;

        self.date.append_value(date_days);
        self.timestamp.append_value(event.timestamp);
        self.hostname.append_value(&event.hostname);
        self.mount_point.append_value(&event.mount_point);
        self.disk_name.append_value(&event.disk_name);
        self.file_system.append_value(&event.file_system);
        self.total_space.append_value(event.total_space as i64);
        self.used_space.append_value(event.used_space as i64);
        self.available_space.append_value(event.available_space as i64);
        self.usage_percent.append_value(event.usage_percent);

        self.count += 1;
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn into_record_batch(mut self) -> arrow::error::Result<RecordBatch> {
        let schema = host_disk_schema();

        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(self.date.finish()),
                Arc::new(self.timestamp.finish().with_timezone("UTC")),
                Arc::new(self.hostname.finish()),
                Arc::new(self.mount_point.finish()),
                Arc::new(self.disk_name.finish()),
                Arc::new(self.file_system.finish()),
                Arc::new(self.total_space.finish()),
                Arc::new(self.used_space.finish()),
                Arc::new(self.available_space.finish()),
                Arc::new(self.usage_percent.finish()),
            ],
        )
    }
}