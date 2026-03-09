use super::super::otlp::LogsData;
use super::super::schema::logs_schema;
use arrow::array::{
    Date32Builder, MapBuilder, MapFieldNames, StringBuilder, TimestampMicrosecondBuilder,
};
use arrow::record_batch::RecordBatch;
use std::sync::Arc;

pub struct LogsBuffer {
    date: Date32Builder,
    timestamp: TimestampMicrosecondBuilder,
    service: StringBuilder,
    severity: StringBuilder,
    body: StringBuilder,
    trace_id: StringBuilder,
    span_id: StringBuilder,
    attributes: MapBuilder<StringBuilder, StringBuilder>,
    resource: MapBuilder<StringBuilder, StringBuilder>,
    count: usize,
}

impl LogsBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            date: Date32Builder::with_capacity(capacity),
            timestamp: TimestampMicrosecondBuilder::with_capacity(capacity),
            service: StringBuilder::with_capacity(capacity, capacity * 20),
            severity: StringBuilder::with_capacity(capacity, capacity * 10),
            body: StringBuilder::with_capacity(capacity, capacity * 100),
            trace_id: StringBuilder::with_capacity(capacity, capacity * 32),
            span_id: StringBuilder::with_capacity(capacity, capacity * 16),
            attributes: MapBuilder::new(
                Some(MapFieldNames {
                    entry: "entries".to_string(),
                    key: "key".to_string(),
                    value: "value".to_string(),
                }),
                StringBuilder::new(),
                StringBuilder::new(),
            ),
            resource: MapBuilder::new(
                Some(MapFieldNames {
                    entry: "entries".to_string(),
                    key: "key".to_string(),
                    value: "value".to_string(),
                }),
                StringBuilder::new(),
                StringBuilder::new(),
            ),
            count: 0,
        }
    }

    pub fn push_logs_data(&mut self, data: &LogsData) {
        for resource_log in &data.resource_logs {
            let mut resource_map = std::collections::HashMap::new();
            if let Some(resource) = &resource_log.resource {
                for kv in &resource.attributes {
                    if let Some(val) = &kv.value.string_value {
                        resource_map.insert(kv.key.clone(), val.clone());
                    }
                }
            }

            let service_name = resource_map
                .get("service.name")
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());

            for scope_log in &resource_log.scope_logs {
                for record in &scope_log.log_records {
                    let timestamp_nanos: i64 = record.time_unix_nano.parse().unwrap_or(0);
                    let timestamp_micros = timestamp_nanos / 1000;
                    let date_days = (timestamp_micros / (86400 * 1_000_000)) as i32;

                    self.date.append_value(date_days);
                    self.timestamp.append_value(timestamp_micros);
                    self.service.append_value(&service_name);
                    self.severity.append_option(record.severity_text.as_ref());

                    if let Some(body) = &record.body {
                        self.body.append_option(body.string_value.as_ref());
                    } else {
                        self.body.append_null();
                    }

                    self.trace_id.append_option(record.trace_id.as_ref());
                    self.span_id.append_option(record.span_id.as_ref());

                    // Attributes
                    if let Some(attrs) = &record.attributes {
                        for kv in attrs {
                            self.attributes.keys().append_value(&kv.key);
                            if let Some(v) = &kv.value.string_value {
                                self.attributes.values().append_value(v);
                            } else {
                                self.attributes.values().append_null();
                            }
                        }
                        self.attributes.append(true).unwrap();
                    } else {
                        self.attributes.append(false).unwrap();
                    }

                    // Resource
                    if !resource_map.is_empty() {
                        for (k, v) in &resource_map {
                            self.resource.keys().append_value(k);
                            self.resource.values().append_value(v);
                        }
                        self.resource.append(true).unwrap();
                    } else {
                        self.resource.append(false).unwrap();
                    }

                    self.count += 1;
                }
            }
        }
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn into_record_batch(mut self) -> arrow::error::Result<RecordBatch> {
        let schema = logs_schema();

        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(self.date.finish()),
                Arc::new(self.timestamp.finish().with_timezone("UTC")),
                Arc::new(self.service.finish()),
                Arc::new(self.severity.finish()),
                Arc::new(self.body.finish()),
                Arc::new(self.trace_id.finish()),
                Arc::new(self.span_id.finish()),
                Arc::new(self.attributes.finish()),
                Arc::new(self.resource.finish()),
            ],
        )
    }
}
