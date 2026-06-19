use super::super::events::HostCpuEvent;
use super::super::schema::host_cpu_schema;
use arrow::array::{
    Date32Builder, Float64Builder, Int32Builder, StringBuilder, TimestampMicrosecondBuilder,
};
use arrow::record_batch::RecordBatch;
use std::sync::Arc;

pub struct HostCpuBuffer {
    date: Date32Builder,
    timestamp: TimestampMicrosecondBuilder,
    hostname: StringBuilder,
    load_avg_1m: Float64Builder,
    load_avg_5m: Float64Builder,
    load_avg_15m: Float64Builder,
    core_count: Int32Builder,

    count: usize,
}

impl HostCpuBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            date: Date32Builder::with_capacity(capacity),
            timestamp: TimestampMicrosecondBuilder::with_capacity(capacity),
            hostname: StringBuilder::with_capacity(capacity, capacity * 64),
            load_avg_1m: Float64Builder::with_capacity(capacity),
            load_avg_5m: Float64Builder::with_capacity(capacity),
            load_avg_15m: Float64Builder::with_capacity(capacity),
            core_count: Int32Builder::with_capacity(capacity),
            count: 0,
        }
    }

    pub fn push(&mut self, event: &HostCpuEvent) {
        let date_days = (event.timestamp / (86400 * 1_000_000)) as i32;

        self.date.append_value(date_days);
        self.timestamp.append_value(event.timestamp);
        self.hostname.append_value(&event.hostname);
        self.load_avg_1m.append_value(event.load_avg_1m);
        self.load_avg_5m.append_value(event.load_avg_5m);
        self.load_avg_15m.append_value(event.load_avg_15m);
        self.core_count.append_value(event.core_count as i32);

        self.count += 1;
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn into_record_batch(mut self) -> arrow::error::Result<RecordBatch> {
        let schema = host_cpu_schema();

        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(self.date.finish()),
                Arc::new(self.timestamp.finish().with_timezone("UTC")),
                Arc::new(self.hostname.finish()),
                Arc::new(self.load_avg_1m.finish()),
                Arc::new(self.load_avg_5m.finish()),
                Arc::new(self.load_avg_15m.finish()),
                Arc::new(self.core_count.finish()),
            ],
        )
    }
}
