use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use deltalake::kernel::{DataType as DeltaDataType, PrimitiveType, StructField};
use deltalake::operations::create::CreateBuilder;
use deltalake::protocol::SaveMode;
use deltalake::{DeltaTable, DeltaTableError, TableProperty};
use std::sync::Arc;

pub async fn create_deployments_table(table_uri: &str) -> Result<DeltaTable, DeltaTableError> {
    let columns = vec![
        StructField::new("date", DeltaDataType::Primitive(PrimitiveType::Date), false),
        StructField::new(
            "timestamp",
            DeltaDataType::Primitive(PrimitiveType::Timestamp),
            false,
        ),
        StructField::new(
            "service",
            DeltaDataType::Primitive(PrimitiveType::String),
            false,
        ),
        StructField::new(
            "image",
            DeltaDataType::Primitive(PrimitiveType::String),
            false,
        ),
        StructField::new(
            "image_sha",
            DeltaDataType::Primitive(PrimitiveType::String),
            false,
        ),
        StructField::new(
            "image_size_mb",
            DeltaDataType::Primitive(PrimitiveType::Long),
            false,
        ),
        StructField::new(
            "container_id",
            DeltaDataType::Primitive(PrimitiveType::String),
            false,
        ),
        StructField::new(
            "container_created_at",
            DeltaDataType::Primitive(PrimitiveType::Timestamp),
            false,
        ),
        StructField::new(
            "trigger_type",
            DeltaDataType::Primitive(PrimitiveType::String),
            false,
        ),
        StructField::new(
            "dispenser_version",
            DeltaDataType::Primitive(PrimitiveType::String),
            false,
        ),
        StructField::new(
            "restart_policy",
            DeltaDataType::Primitive(PrimitiveType::String),
            false,
        ),
        StructField::new(
            "memory_limit",
            DeltaDataType::Primitive(PrimitiveType::String),
            true,
        ),
        StructField::new(
            "cpu_limit",
            DeltaDataType::Primitive(PrimitiveType::String),
            true,
        ),
        StructField::new(
            "proxy_enabled",
            DeltaDataType::Primitive(PrimitiveType::Boolean),
            false,
        ),
        StructField::new(
            "proxy_host",
            DeltaDataType::Primitive(PrimitiveType::String),
            true,
        ),
        StructField::new(
            "port_mappings_count",
            DeltaDataType::Primitive(PrimitiveType::Integer),
            false,
        ),
        StructField::new(
            "volume_count",
            DeltaDataType::Primitive(PrimitiveType::Integer),
            false,
        ),
        StructField::new(
            "network_count",
            DeltaDataType::Primitive(PrimitiveType::Integer),
            false,
        ),
    ];

    CreateBuilder::new()
        .with_location(table_uri)
        .with_columns(columns)
        .with_partition_columns(vec!["date"])
        .with_save_mode(SaveMode::Ignore)
        .with_configuration_property(
            TableProperty::LogRetentionDuration,
            Some("interval 30 days"),
        )
        .with_configuration_property(
            TableProperty::DeletedFileRetentionDuration,
            Some("interval 7 days"),
        )
        // 32MB target file size (good for streaming ingestion)
        .with_configuration_property(TableProperty::TargetFileSize, Some("33554432"))
        .await
}

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
