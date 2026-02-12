use super::super::events::ContainerStatusEvent;
use super::super::schema::status_schema;
use arrow::array::{
    Date32Builder, Int32Builder, Int64Builder, StringBuilder, StringDictionaryBuilder,
    TimestampMicrosecondBuilder,
};
use arrow::datatypes::Int8Type;
use arrow::record_batch::RecordBatch;
use std::sync::Arc;

pub struct StatusBuffer {
    date: Date32Builder,
    timestamp: TimestampMicrosecondBuilder,
    service: StringBuilder,
    container_id: StringBuilder,
    state: StringDictionaryBuilder<Int8Type>,
    health_status: StringDictionaryBuilder<Int8Type>,
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
            state: StringDictionaryBuilder::new(),
            health_status: StringDictionaryBuilder::new(),
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
        self.state.append_value(event.state.as_ref());
        self.health_status
            .append_value(event.health_status.as_ref());

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
