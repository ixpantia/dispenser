use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use deltalake::kernel::{DataType as DeltaDataType, PrimitiveType, StructField};
use deltalake::operations::create::CreateBuilder;
use deltalake::protocol::SaveMode;
use deltalake::{DeltaTable, DeltaTableError, TableProperty};
use std::sync::Arc;

pub async fn create_logs_table(table_uri: &str) -> Result<DeltaTable, DeltaTableError> {
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
            "severity",
            DeltaDataType::Primitive(PrimitiveType::String),
            true,
        ),
        StructField::new(
            "body",
            DeltaDataType::Primitive(PrimitiveType::String),
            true,
        ),
        StructField::new(
            "trace_id",
            DeltaDataType::Primitive(PrimitiveType::String),
            true,
        ),
        StructField::new(
            "span_id",
            DeltaDataType::Primitive(PrimitiveType::String),
            true,
        ),
        StructField::new(
            "attributes",
            DeltaDataType::Primitive(PrimitiveType::String),
            true,
        ),
        StructField::new(
            "resource",
            DeltaDataType::Primitive(PrimitiveType::String),
            true,
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
        .with_configuration_property(TableProperty::TargetFileSize, Some("33554432"))
        .await
}

pub fn logs_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("date", DataType::Date32, false),
        Field::new(
            "timestamp",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new("service", DataType::Utf8, false),
        Field::new("severity", DataType::Utf8, true),
        Field::new("body", DataType::Utf8, true),
        Field::new("trace_id", DataType::Utf8, true),
        Field::new("span_id", DataType::Utf8, true),
        Field::new("attributes", DataType::Utf8, true),
        Field::new("resource", DataType::Utf8, true),
    ]))
}
