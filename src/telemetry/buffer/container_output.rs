use super::super::events::ContainerOutputEvent;
use super::super::schema::container_output_schema;
use arrow::array::{Date32Builder, Int64Builder, StringBuilder, TimestampMicrosecondBuilder};
use arrow::record_batch::RecordBatch;
use std::sync::Arc;

pub struct ContainerOutputBuffer {
    date: Date32Builder,
    timestamp: TimestampMicrosecondBuilder,
    service: StringBuilder,
    container_id: StringBuilder,
    stream: StringBuilder,
    message: StringBuilder,
    sequence: Int64Builder,
    count: usize,
}

impl ContainerOutputBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            date: Date32Builder::with_capacity(capacity),
            timestamp: TimestampMicrosecondBuilder::with_capacity(capacity),
            service: StringBuilder::with_capacity(capacity, capacity * 20),
            container_id: StringBuilder::with_capacity(capacity, capacity * 64),
            stream: StringBuilder::with_capacity(capacity, capacity * 10),
            message: StringBuilder::with_capacity(capacity, capacity * 200),
            sequence: Int64Builder::with_capacity(capacity),
            count: 0,
        }
    }

    pub fn push(&mut self, event: &ContainerOutputEvent) {
        let date_days = (event.timestamp / (86400 * 1_000_000)) as i32;

        self.date.append_value(date_days);
        self.timestamp.append_value(event.timestamp);
        self.service.append_value(&event.service);
        self.container_id.append_value(&event.container_id);
        self.stream.append_value(&event.stream);
        self.message.append_value(&event.message);
        self.sequence.append_value(event.sequence);

        self.count += 1;
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn into_record_batch(mut self) -> arrow::error::Result<RecordBatch> {
        let schema = container_output_schema();

        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(self.date.finish()),
                Arc::new(self.timestamp.finish().with_timezone("UTC")),
                Arc::new(self.service.finish()),
                Arc::new(self.container_id.finish()),
                Arc::new(self.stream.finish()),
                Arc::new(self.message.finish()),
                Arc::new(self.sequence.finish()),
            ],
        )
    }
}
