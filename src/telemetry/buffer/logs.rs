use super::super::schema::logs_schema;
use super::json::key_values_to_json;
use arrow::array::{Date32Builder, StringBuilder, TimestampMicrosecondBuilder};
use arrow::record_batch::RecordBatch;
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::common::v1::any_value;
use std::sync::Arc;

pub struct LogsBuffer {
    date: Date32Builder,
    timestamp: TimestampMicrosecondBuilder,
    service: StringBuilder,
    severity: StringBuilder,
    body: StringBuilder,
    trace_id: StringBuilder,
    span_id: StringBuilder,
    attributes: StringBuilder,
    resource: StringBuilder,
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
            attributes: StringBuilder::with_capacity(capacity, capacity * 200),
            resource: StringBuilder::with_capacity(capacity, capacity * 200),
            count: 0,
        }
    }

    pub fn push_logs_data(&mut self, data: &ExportLogsServiceRequest) {
        for resource_log in &data.resource_logs {
            let mut service_name = "unknown".to_string();
            let mut resource_json = None;

            if let Some(resource) = &resource_log.resource {
                for kv in &resource.attributes {
                    if kv.key == "service.name" {
                        if let Some(v) = &kv.value {
                            if let Some(any_value::Value::StringValue(s)) = &v.value {
                                service_name = s.clone();
                            }
                        }
                    }
                }
                resource_json = key_values_to_json(&resource.attributes);
            }

            for scope_log in &resource_log.scope_logs {
                for record in &scope_log.log_records {
                    let timestamp_nanos: i64 = record.time_unix_nano as i64;
                    let timestamp_micros = timestamp_nanos / 1000;
                    let date_days = (timestamp_micros / (86400 * 1_000_000)) as i32;

                    self.date.append_value(date_days);
                    self.timestamp.append_value(timestamp_micros);
                    self.service.append_value(&service_name);
                    if record.severity_text.is_empty() {
                        self.severity.append_null();
                    } else {
                        self.severity.append_value(&record.severity_text);
                    }

                    if let Some(body) = &record.body {
                        match &body.value {
                            Some(any_value::Value::StringValue(s)) => self.body.append_value(s),
                            Some(any_value::Value::IntValue(i)) => {
                                self.body.append_value(i.to_string())
                            }
                            Some(any_value::Value::DoubleValue(d)) => {
                                self.body.append_value(d.to_string())
                            }
                            Some(any_value::Value::BoolValue(b)) => {
                                self.body.append_value(b.to_string())
                            }
                            Some(v @ any_value::Value::ArrayValue(_))
                            | Some(v @ any_value::Value::KvlistValue(_)) => {
                                let json_val = super::json::any_value_to_json(v);
                                self.body.append_value(json_val.to_string())
                            }
                            Some(any_value::Value::BytesValue(b)) => {
                                let encoded: String =
                                    b.iter().map(|byte| format!("{:02x}", byte)).collect();
                                self.body.append_value(encoded)
                            }
                            None => self.body.append_null(),
                        }
                    } else {
                        self.body.append_null();
                    }

                    if record.trace_id.is_empty() {
                        self.trace_id.append_null();
                    } else {
                        let trace_id_hex = record
                            .trace_id
                            .iter()
                            .map(|b| format!("{:02x}", b))
                            .collect::<String>();
                        self.trace_id.append_value(&trace_id_hex);
                    }

                    if record.span_id.is_empty() {
                        self.span_id.append_null();
                    } else {
                        let span_id_hex = record
                            .span_id
                            .iter()
                            .map(|b| format!("{:02x}", b))
                            .collect::<String>();
                        self.span_id.append_value(&span_id_hex);
                    }

                    // Attributes
                    if let Some(json_str) = key_values_to_json(&record.attributes) {
                        self.attributes.append_value(json_str);
                    } else {
                        self.attributes.append_null();
                    }

                    // Resource
                    if let Some(json_str) = &resource_json {
                        self.resource.append_value(json_str);
                    } else {
                        self.resource.append_null();
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

    pub fn into_record_batch(&mut self) -> arrow::error::Result<RecordBatch> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry_proto::tonic::common::v1::{AnyValue, KeyValue};
    use opentelemetry_proto::tonic::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
    use opentelemetry_proto::tonic::resource::v1::Resource;

    #[test]
    fn test_push_logs_data() {
        let mut buffer = LogsBuffer::new(10);
        let mut data = ExportLogsServiceRequest::default();

        let mut record = LogRecord::default();
        record.time_unix_nano = 1680000000000000000;
        record.severity_text = "INFO".to_string();
        record.body = Some(AnyValue {
            value: Some(any_value::Value::StringValue(
                "test log message".to_string(),
            )),
        });

        record.trace_id = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        record.span_id = vec![1, 2, 3, 4, 5, 6, 7, 8];

        let mut scope_log = ScopeLogs::default();
        scope_log.log_records.push(record);

        let mut resource = Resource::default();
        resource.attributes.push(KeyValue {
            key: "service.name".to_string(),
            value: Some(AnyValue {
                value: Some(any_value::Value::StringValue("test-service".to_string())),
            }),
        });

        let mut resource_log = ResourceLogs::default();
        resource_log.resource = Some(resource);
        resource_log.scope_logs.push(scope_log);

        data.resource_logs.push(resource_log);

        assert!(buffer.is_empty());
        assert_eq!(buffer.len(), 0);

        buffer.push_logs_data(&data);

        assert!(!buffer.is_empty());
        assert_eq!(buffer.len(), 1);

        let batch = buffer.into_record_batch().unwrap();
        assert_eq!(batch.num_rows(), 1);

        let service_col = batch
            .column(2)
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .unwrap();
        assert_eq!(service_col.value(0), "test-service");

        let severity_col = batch
            .column(3)
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .unwrap();
        assert_eq!(severity_col.value(0), "INFO");

        let body_col = batch
            .column(4)
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .unwrap();
        assert_eq!(body_col.value(0), "test log message");

        let trace_id_col = batch
            .column(5)
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .unwrap();
        assert_eq!(trace_id_col.value(0), "0102030405060708090a0b0c0d0e0f10");

        let span_id_col = batch
            .column(6)
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .unwrap();
        assert_eq!(span_id_col.value(0), "0102030405060708");
    }
}
