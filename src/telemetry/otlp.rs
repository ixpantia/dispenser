use serde::{Deserialize, Serialize};

/// Represents the top-level OTLP Logs request.
/// https://github.com/open-telemetry/opentelemetry-proto/blob/main/opentelemetry/proto/logs/v1/logs.proto
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogsData {
    #[serde(rename = "resourceLogs")]
    pub resource_logs: Vec<ResourceLogs>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLogs {
    pub resource: Option<Resource>,
    #[serde(rename = "scopeLogs")]
    pub scope_logs: Vec<ScopeLogs>,
    #[serde(rename = "schemaUrl")]
    pub schema_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeLogs {
    pub scope: Option<InstrumentationScope>,
    #[serde(rename = "logRecords")]
    pub log_records: Vec<LogRecord>,
    #[serde(rename = "schemaUrl")]
    pub schema_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRecord {
    #[serde(rename = "timeUnixNano")]
    pub time_unix_nano: String, // OTLP JSON uses strings for uint64 to avoid precision loss
    #[serde(rename = "observedTimeUnixNano")]
    pub observed_time_unix_nano: Option<String>,
    #[serde(rename = "severityNumber")]
    pub severity_number: Option<i32>,
    #[serde(rename = "severityText")]
    pub severity_text: Option<String>,
    pub body: Option<AnyValue>,
    pub attributes: Option<Vec<KeyValue>>,
    #[serde(rename = "droppedAttributesCount")]
    pub dropped_attributes_count: Option<u32>,
    pub flags: Option<u32>,
    #[serde(rename = "traceId")]
    pub trace_id: Option<String>,
    #[serde(rename = "spanId")]
    pub span_id: Option<String>,
}

/// Represents the top-level OTLP Traces request.
/// https://github.com/open-telemetry/opentelemetry-proto/blob/main/opentelemetry/proto/trace/v1/trace.proto
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracesData {
    #[serde(rename = "resourceSpans")]
    pub resource_spans: Vec<ResourceSpans>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSpans {
    pub resource: Option<Resource>,
    #[serde(rename = "scopeSpans")]
    pub scope_spans: Vec<ScopeSpans>,
    #[serde(rename = "schemaUrl")]
    pub schema_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeSpans {
    pub scope: Option<InstrumentationScope>,
    pub spans: Vec<Span>,
    #[serde(rename = "schemaUrl")]
    pub schema_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    #[serde(rename = "traceId")]
    pub trace_id: String,
    #[serde(rename = "spanId")]
    pub span_id: String,
    #[serde(rename = "traceState")]
    pub trace_state: Option<String>,
    #[serde(rename = "parentSpanId")]
    pub parent_span_id: Option<String>,
    pub name: String,
    pub kind: i32,
    #[serde(rename = "startTimeUnixNano")]
    pub start_time_unix_nano: String,
    #[serde(rename = "endTimeUnixNano")]
    pub end_time_unix_nano: String,
    pub attributes: Option<Vec<KeyValue>>,
    #[serde(rename = "droppedAttributesCount")]
    pub dropped_attributes_count: Option<u32>,
    pub events: Option<Vec<SpanEvent>>,
    #[serde(rename = "droppedEventsCount")]
    pub dropped_events_count: Option<u32>,
    pub links: Option<Vec<SpanLink>>,
    #[serde(rename = "droppedLinksCount")]
    pub dropped_links_count: Option<u32>,
    pub status: Option<Status>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanEvent {
    #[serde(rename = "timeUnixNano")]
    pub time_unix_nano: String,
    pub name: String,
    pub attributes: Option<Vec<KeyValue>>,
    #[serde(rename = "droppedAttributesCount")]
    pub dropped_attributes_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanLink {
    #[serde(rename = "traceId")]
    pub trace_id: String,
    #[serde(rename = "spanId")]
    pub span_id: String,
    #[serde(rename = "traceState")]
    pub trace_state: Option<String>,
    pub attributes: Option<Vec<KeyValue>>,
    #[serde(rename = "droppedAttributesCount")]
    pub dropped_attributes_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    pub message: Option<String>,
    pub code: i32,
}

/// Common types used by both Logs and Traces.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub attributes: Vec<KeyValue>,
    #[serde(rename = "droppedAttributesCount")]
    pub dropped_attributes_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentationScope {
    pub name: String,
    pub version: Option<String>,
    pub attributes: Option<Vec<KeyValue>>,
    #[serde(rename = "droppedAttributesCount")]
    pub dropped_attributes_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyValue {
    pub key: String,
    pub value: AnyValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnyValue {
    #[serde(rename = "stringValue")]
    pub string_value: Option<String>,
    #[serde(rename = "boolValue")]
    pub bool_value: Option<bool>,
    #[serde(rename = "intValue")]
    pub int_value: Option<String>, // string for int64
    #[serde(rename = "doubleValue")]
    pub double_value: Option<f64>,
    #[serde(rename = "arrayValue")]
    pub array_value: Option<ArrayValue>,
    #[serde(rename = "kvlistValue")]
    pub kvlist_value: Option<KeyValueList>,
    #[serde(rename = "bytesValue")]
    pub bytes_value: Option<String>, // base64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArrayValue {
    pub values: Vec<AnyValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyValueList {
    pub values: Vec<KeyValue>,
}
