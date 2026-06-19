use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use deltalake::kernel::{DataType as DeltaDataType, PrimitiveType, StructField};
use deltalake::operations::create::CreateBuilder;
use deltalake::protocol::SaveMode;
use deltalake::{DeltaTable, DeltaTableError, TableProperty};
use std::sync::Arc;
use url::Url;

pub async fn create_host_cpu_table(table_uri: &Url) -> Result<DeltaTable, DeltaTableError> {
    let columns = vec![
        StructField::new("date", DeltaDataType::Primitive(PrimitiveType::Date), false),
        StructField::new(
            "timestamp",
            DeltaDataType::Primitive(PrimitiveType::Timestamp),
            false,
        ),
        StructField::new(
            "hostname",
            DeltaDataType::Primitive(PrimitiveType::String),
            false,
        ),
        StructField::new(
            "load_avg_1m",
            DeltaDataType::Primitive(PrimitiveType::Double),
            false,
        ),
        StructField::new(
            "load_avg_5m",
            DeltaDataType::Primitive(PrimitiveType::Double),
            false,
        ),
        StructField::new(
            "load_avg_15m",
            DeltaDataType::Primitive(PrimitiveType::Double),
            false,
        ),
        StructField::new(
            "core_count",
            DeltaDataType::Primitive(PrimitiveType::Integer),
            false,
        ),
    ];

    CreateBuilder::new()
        .with_location(table_uri.as_str())
        .with_columns(columns)
        .with_partition_columns(vec!["date"])
        .with_save_mode(SaveMode::Ignore)
        .with_configuration_property(
            TableProperty::DeletedFileRetentionDuration,
            Some("interval 1 days"),
        )
        .with_configuration_property(
            TableProperty::LogRetentionDuration,
            Some("interval 1 hours"),
        )
        .with_configuration_property(TableProperty::CheckpointInterval, Some("20"))
        .with_configuration_property(TableProperty::TargetFileSize, Some("128mb"))
        .await
}

pub fn host_cpu_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("date", DataType::Date32, false),
        Field::new(
            "timestamp",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new("hostname", DataType::Utf8, false),
        Field::new("load_avg_1m", DataType::Float64, false),
        Field::new("load_avg_5m", DataType::Float64, false),
        Field::new("load_avg_15m", DataType::Float64, false),
        Field::new("core_count", DataType::Int32, false),
    ]))
}
