use super::bytes_to_hex;

use super::super::schema::traces_schema;
use super::json::key_values_to_json;
use arrow::array::{
    Date32Builder, Int64Builder, ListBuilder, StringBuilder, StructBuilder,
    TimestampMicrosecondBuilder,
};
use arrow::datatypes::{DataType, Field, TimeUnit};
use arrow::record_batch::RecordBatch;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::any_value;
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
    attributes: StringBuilder,
    events: ListBuilder<StructBuilder>,
    count: usize,
}

impl SpansBuffer {
    pub fn new(capacity: usize) -> Self {
        let events_fields = vec![
            Field::new(
                "timestamp",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("name", DataType::Utf8, false),
            Field::new("attributes", DataType::Utf8, true),
        ];

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
            attributes: StringBuilder::with_capacity(capacity, capacity * 200),
            events: ListBuilder::new(StructBuilder::from_fields(events_fields, capacity)),
            count: 0,
        }
    }

    pub fn push_traces_data(&mut self, data: &ExportTraceServiceRequest) {
        for resource_span in &data.resource_spans {
            let mut service_name = "unknown".to_string();

            if let Some(resource) = &resource_span.resource {
                for kv in &resource.attributes {
                    if kv.key == "service.name" {
                        if let Some(v) = &kv.value {
                            if let Some(any_value::Value::StringValue(s)) = &v.value {
                                service_name = s.clone();
                            }
                        }
                    }
                }
            }

            for scope_span in &resource_span.scope_spans {
                for span in &scope_span.spans {
                    let start_nanos: i64 = span.start_time_unix_nano as i64;
                    let end_nanos: i64 = span.end_time_unix_nano as i64;
                    let start_micros = start_nanos / 1000;
                    let end_micros = end_nanos / 1000;
                    let duration_ms = (end_nanos - start_nanos) / 1_000_000;
                    let date_days = (start_micros / (86400 * 1_000_000)) as i32;

                    self.date.append_value(date_days);
                    if span.trace_id.is_empty() {
                        self.trace_id.append_value("");
                    } else {
                        let trace_id_hex = bytes_to_hex(&span.trace_id);
                        self.trace_id.append_value(&trace_id_hex);
                    }

                    if span.span_id.is_empty() {
                        self.span_id.append_value("");
                    } else {
                        let span_id_hex = bytes_to_hex(&span.span_id);
                        self.span_id.append_value(&span_id_hex);
                    }

                    if span.parent_span_id.is_empty() {
                        self.parent_span_id.append_null();
                    } else {
                        let parent_span_id_hex = bytes_to_hex(&span.parent_span_id);
                        self.parent_span_id.append_value(&parent_span_id_hex);
                    }

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
                        if status.message.is_empty() {
                            self.status_message.append_null();
                        } else {
                            self.status_message.append_value(&status.message);
                        }
                    } else {
                        self.status_code.append_null();
                        self.status_message.append_null();
                    }

                    self.service.append_value(&service_name);

                    // Attributes
                    if let Some(json_str) = key_values_to_json(&span.attributes) {
                        self.attributes.append_value(json_str);
                    } else {
                        self.attributes.append_null();
                    }

                    // Events
                    if !span.events.is_empty() {
                        let struct_builder = self.events.values();
                        for event in &span.events {
                            let ts_micros = (event.time_unix_nano as i64) / 1000;
                            struct_builder
                                .field_builder::<TimestampMicrosecondBuilder>(0)
                                .unwrap()
                                .append_value(ts_micros);
                            struct_builder
                                .field_builder::<StringBuilder>(1)
                                .unwrap()
                                .append_value(&event.name);
                            if let Some(json_str) = key_values_to_json(&event.attributes) {
                                struct_builder
                                    .field_builder::<StringBuilder>(2)
                                    .unwrap()
                                    .append_value(json_str);
                            } else {
                                struct_builder
                                    .field_builder::<StringBuilder>(2)
                                    .unwrap()
                                    .append_null();
                            }
                            struct_builder.append(true);
                        }
                        self.events.append(true);
                    } else {
                        self.events.append(false);
                    }

                    self.count += 1;
                }
            }
        }
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
                Arc::new(self.events.finish()),
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
    use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span, span::Event};

    #[test]
    fn test_push_traces_data() {
        let mut buffer = SpansBuffer::new(10);
        let mut data = ExportTraceServiceRequest::default();

        let mut span = Span::default();
        span.trace_id = vec![1, 2, 3, 4];
        span.span_id = vec![5, 6, 7, 8];
        span.name = "test_span".to_string();

        let mut event = Event::default();
        event.time_unix_nano = 1000000;
        event.name = "test_event".to_string();
        span.events.push(event);

        let mut scope_spans = ScopeSpans::default();
        scope_spans.spans.push(span);

        let mut resource_spans = ResourceSpans::default();
        resource_spans.scope_spans.push(scope_spans);

        data.resource_spans.push(resource_spans);

        buffer.push_traces_data(&data);
        assert_eq!(buffer.count, 1);

        let batch = buffer.into_record_batch().unwrap();
        assert_eq!(batch.num_rows(), 1);
    }
}
