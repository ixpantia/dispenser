use super::super::events::HostMemoryEvent;
use super::super::schema::host_memory_schema;
use arrow::array::{
    Date32Builder, Float64Builder, Int64Builder, StringBuilder, TimestampMicrosecondBuilder,
};
use arrow::record_batch::RecordBatch;
use std::sync::Arc;

pub struct HostMemoryBuffer {
    date: Date32Builder,
    timestamp: TimestampMicrosecondBuilder,
    hostname: StringBuilder,
    total_memory: Int64Builder,
    used_memory: Int64Builder,
    available_memory: Int64Builder,
    memory_usage_percent: Float64Builder,
    total_swap: Int64Builder,
    used_swap: Int64Builder,
    swap_usage_percent: Float64Builder,

    count: usize,
}

impl HostMemoryBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            date: Date32Builder::with_capacity(capacity),
            timestamp: TimestampMicrosecondBuilder::with_capacity(capacity),
            hostname: StringBuilder::with_capacity(capacity, capacity * 64),
            total_memory: Int64Builder::with_capacity(capacity),
            used_memory: Int64Builder::with_capacity(capacity),
            available_memory: Int64Builder::with_capacity(capacity),
            memory_usage_percent: Float64Builder::with_capacity(capacity),
            total_swap: Int64Builder::with_capacity(capacity),
            used_swap: Int64Builder::with_capacity(capacity),
            swap_usage_percent: Float64Builder::with_capacity(capacity),
            count: 0,
        }
    }

    pub fn push(&mut self, event: &HostMemoryEvent) {
        let date_days = (event.timestamp / (86400 * 1_000_000)) as i32;

        self.date.append_value(date_days);
        self.timestamp.append_value(event.timestamp);
        self.hostname.append_value(&event.hostname);
        self.total_memory.append_value(event.total_memory as i64);
        self.used_memory.append_value(event.used_memory as i64);
        self.available_memory
            .append_value(event.available_memory as i64);
        self.memory_usage_percent
            .append_value(event.memory_usage_percent);
        self.total_swap.append_value(event.total_swap as i64);
        self.used_swap.append_value(event.used_swap as i64);
        self.swap_usage_percent
            .append_value(event.swap_usage_percent);

        self.count += 1;
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn into_record_batch(mut self) -> arrow::error::Result<RecordBatch> {
        let schema = host_memory_schema();

        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(self.date.finish()),
                Arc::new(self.timestamp.finish().with_timezone("UTC")),
                Arc::new(self.hostname.finish()),
                Arc::new(self.total_memory.finish()),
                Arc::new(self.used_memory.finish()),
                Arc::new(self.available_memory.finish()),
                Arc::new(self.memory_usage_percent.finish()),
                Arc::new(self.total_swap.finish()),
                Arc::new(self.used_swap.finish()),
                Arc::new(self.swap_usage_percent.finish()),
            ],
        )
    }
}
