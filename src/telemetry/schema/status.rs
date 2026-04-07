use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use deltalake::kernel::{DataType as DeltaDataType, PrimitiveType, StructField};
use deltalake::operations::create::CreateBuilder;
use deltalake::protocol::SaveMode;
use deltalake::{DeltaTable, DeltaTableError, TableProperty};
use std::sync::Arc;
use url::Url;

pub async fn create_status_table(table_uri: &Url) -> Result<DeltaTable, DeltaTableError> {
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
            "container_id",
            DeltaDataType::Primitive(PrimitiveType::String),
            false,
        ),
        StructField::new(
            "state",
            DeltaDataType::Primitive(PrimitiveType::String),
            false,
        ),
        StructField::new(
            "health_status",
            DeltaDataType::Primitive(PrimitiveType::String),
            false,
        ),
        StructField::new(
            "exit_code",
            DeltaDataType::Primitive(PrimitiveType::Integer),
            true,
        ),
        StructField::new(
            "restart_count",
            DeltaDataType::Primitive(PrimitiveType::Integer),
            false,
        ),
        StructField::new(
            "uptime_seconds",
            DeltaDataType::Primitive(PrimitiveType::Long),
            false,
        ),
        StructField::new(
            "failing_streak",
            DeltaDataType::Primitive(PrimitiveType::Integer),
            false,
        ),
        StructField::new(
            "last_health_output",
            DeltaDataType::Primitive(PrimitiveType::String),
            true,
        ),
    ];

    CreateBuilder::new()
        .with_location(table_uri.as_str())
        .with_columns(columns)
        .with_partition_columns(vec!["date"])
        .with_save_mode(SaveMode::Ignore)
        .with_configuration_property(
            TableProperty::DeletedFileRetentionDuration,
            Some("interval 7 days"),
        )
        .with_configuration_property(
            TableProperty::LogRetentionDuration,
            Some("interval 1 hours"),
        )
        .with_configuration_property(TableProperty::AutoOptimizeAutoCompact, Some("true"))
        .with_configuration_property(TableProperty::AutoOptimizeOptimizeWrite, Some("true"))
        .with_configuration_property(TableProperty::CheckpointInterval, Some("20"))
        .with_configuration_property(TableProperty::TargetFileSize, Some("128mb"))
        .await
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
