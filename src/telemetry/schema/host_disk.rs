use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use deltalake::kernel::{DataType as DeltaDataType, PrimitiveType, StructField};
use deltalake::operations::create::CreateBuilder;
use deltalake::protocol::SaveMode;
use deltalake::{DeltaTable, DeltaTableError, TableProperty};
use std::sync::Arc;
use url::Url;

pub async fn create_host_disk_table(table_uri: &Url) -> Result<DeltaTable, DeltaTableError> {
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
            "mount_point",
            DeltaDataType::Primitive(PrimitiveType::String),
            false,
        ),
        StructField::new(
            "disk_name",
            DeltaDataType::Primitive(PrimitiveType::String),
            false,
        ),
        StructField::new(
            "file_system",
            DeltaDataType::Primitive(PrimitiveType::String),
            false,
        ),
        StructField::new(
            "total_space",
            DeltaDataType::Primitive(PrimitiveType::Long),
            false,
        ),
        StructField::new(
            "used_space",
            DeltaDataType::Primitive(PrimitiveType::Long),
            false,
        ),
        StructField::new(
            "available_space",
            DeltaDataType::Primitive(PrimitiveType::Long),
            false,
        ),
        StructField::new(
            "usage_percent",
            DeltaDataType::Primitive(PrimitiveType::Double),
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

pub fn host_disk_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("date", DataType::Date32, false),
        Field::new(
            "timestamp",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new("hostname", DataType::Utf8, false),
        Field::new("mount_point", DataType::Utf8, false),
        Field::new("disk_name", DataType::Utf8, false),
        Field::new("file_system", DataType::Utf8, false),
        Field::new("total_space", DataType::Int64, false),
        Field::new("used_space", DataType::Int64, false),
        Field::new("available_space", DataType::Int64, false),
        Field::new("usage_percent", DataType::Float64, false),
    ]))
}
