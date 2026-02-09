use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use std::sync::Arc;

pub fn deployments_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("date", DataType::Date32, false),
        Field::new(
            "timestamp",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new("service", DataType::Utf8, false),
        Field::new("image", DataType::Utf8, false),
        Field::new("image_sha", DataType::Utf8, false),
        Field::new("image_size_mb", DataType::Int64, false),
        Field::new("container_id", DataType::Utf8, false),
        Field::new(
            "container_created_at",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new(
            "trigger_type",
            DataType::Dictionary(Box::new(DataType::Int8), Box::new(DataType::Utf8)),
            false,
        ),
        Field::new("dispenser_version", DataType::Utf8, false),
        Field::new(
            "restart_policy",
            DataType::Dictionary(Box::new(DataType::Int8), Box::new(DataType::Utf8)),
            false,
        ),
        Field::new("memory_limit", DataType::Utf8, true),
        Field::new("cpu_limit", DataType::Utf8, true),
        Field::new("proxy_enabled", DataType::Boolean, false),
        Field::new("proxy_host", DataType::Utf8, true),
        Field::new("port_mappings_count", DataType::Int32, false),
        Field::new("volume_count", DataType::Int32, false),
        Field::new("network_count", DataType::Int32, false),
    ]))
}

pub fn status_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("date", DataType::Date32, false),
        Field::new(
            "timestamp",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new("service", DataType::Utf8, false),
        Field::new("container_id", DataType::Utf8, false),
        Field::new(
            "state",
            DataType::Dictionary(Box::new(DataType::Int8), Box::new(DataType::Utf8)),
            false,
        ),
        Field::new(
            "health_status",
            DataType::Dictionary(Box::new(DataType::Int8), Box::new(DataType::Utf8)),
            false,
        ),
        Field::new("exit_code", DataType::Int32, true),
        Field::new("restart_count", DataType::Int32, false),
        Field::new("uptime_seconds", DataType::Int64, false),
        Field::new("failing_streak", DataType::Int32, false),
        Field::new("last_health_output", DataType::Utf8, true),
    ]))
}
