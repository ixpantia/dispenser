use super::super::otlp::TracesData;
use super::super::schema::traces_schema;
use arrow::array::{
    new_null_array, Date32Builder, Int64Builder, MapBuilder, MapFieldNames, StringBuilder,
    TimestampMicrosecondBuilder,
};
use arrow::record_batch::RecordBatch;
use std::sync::Arc;

pub struct SpansBuffer {
    date: Date32Builder,
    trace_id: StringBuilder,
    span_id: StringBuilder,
    parent_span_id: StringBuilder,
    name: StringBuilder,
    kind: StringBuilder,
    start_time: TimestampMicrosecondBuilder,
    end_time: TimestampMicrosecondBuilder,
    duration_ms: Int64Builder,
    status_code: StringBuilder,
    status_message: StringBuilder,
    service: StringBuilder,
    attributes: MapBuilder<StringBuilder, StringBuilder>,
    count: usize,
}

impl SpansBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            date: Date32Builder::with_capacity(capacity),
            trace_id: StringBuilder::with_capacity(capacity, capacity * 32),
            span_id: StringBuilder::with_capacity(capacity, capacity * 16),
            parent_span_id: StringBuilder::with_capacity(capacity, capacity * 16),
            name: StringBuilder::with_capacity(capacity, capacity * 50),
            kind: StringBuilder::with_capacity(capacity, capacity * 10),
            start_time: TimestampMicrosecondBuilder::with_capacity(capacity),
            end_time: TimestampMicrosecondBuilder::with_capacity(capacity),
            duration_ms: Int64Builder::with_capacity(capacity),
            status_code: StringBuilder::with_capacity(capacity, capacity * 10),
            status_message: StringBuilder::with_capacity(capacity, capacity * 50),
            service: StringBuilder::with_capacity(capacity, capacity * 20),
            attributes: MapBuilder::new(
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

    pub fn push_traces_data(&mut self, data: &TracesData) {
        for resource_span in &data.resource_spans {
            let mut resource_map = std::collections::HashMap::new();
            if let Some(resource) = &resource_span.resource {
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

            for scope_span in &resource_span.scope_spans {
                for span in &scope_span.spans {
                    let start_nanos: i64 = span.start_time_unix_nano.parse().unwrap_or(0);
                    let end_nanos: i64 = span.end_time_unix_nano.parse().unwrap_or(0);
                    let start_micros = start_nanos / 1000;
                    let end_micros = end_nanos / 1000;
                    let duration_ms = (end_nanos - start_nanos) / 1_000_000;
                    let date_days = (start_micros / (86400 * 1_000_000)) as i32;

                    self.date.append_value(date_days);
                    self.trace_id.append_value(&span.trace_id);
                    self.span_id.append_value(&span.span_id);
                    self.parent_span_id
                        .append_option(span.parent_span_id.as_ref());
                    self.name.append_value(&span.name);

                    let kind_str = match span.kind {
                        1 => "INTERNAL",
                        2 => "SERVER",
                        3 => "CLIENT",
                        4 => "PRODUCER",
                        5 => "CONSUMER",
                        _ => "UNSPECIFIED",
                    };
                    self.kind.append_value(kind_str);

                    self.start_time.append_value(start_micros);
                    self.end_time.append_value(end_micros);
                    self.duration_ms.append_value(duration_ms);

                    if let Some(status) = &span.status {
                        let code_str = match status.code {
                            0 => "UNSET",
                            1 => "OK",
                            2 => "ERROR",
                            _ => "UNKNOWN",
                        };
                        self.status_code.append_value(code_str);
                        self.status_message.append_option(status.message.as_ref());
                    } else {
                        self.status_code.append_null();
                        self.status_message.append_null();
                    }

                    self.service.append_value(&service_name);

                    // Attributes
                    if let Some(attrs) = &span.attributes {
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
        let schema = traces_schema();

        RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(self.date.finish()),
                Arc::new(self.trace_id.finish()),
                Arc::new(self.span_id.finish()),
                Arc::new(self.parent_span_id.finish()),
                Arc::new(self.name.finish()),
                Arc::new(self.kind.finish()),
                Arc::new(self.start_time.finish().with_timezone("UTC")),
                Arc::new(self.end_time.finish().with_timezone("UTC")),
                Arc::new(self.duration_ms.finish()),
                Arc::new(self.status_code.finish()),
                Arc::new(self.status_message.finish()),
                Arc::new(self.service.finish()),
                Arc::new(self.attributes.finish()),
                // Note: events are skipped in this version
                Arc::new(new_null_array(
                    schema.field_with_name("events").unwrap().data_type(),
                    self.count,
                )),
            ],
        )
    }
}
