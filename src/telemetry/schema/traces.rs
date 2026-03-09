use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use deltalake::kernel::{
    ArrayType, DataType as DeltaDataType, MapType, PrimitiveType, StructField, StructType,
};
use deltalake::operations::create::CreateBuilder;
use deltalake::protocol::SaveMode;
use deltalake::{DeltaTable, DeltaTableError, TableProperty};
use std::sync::Arc;

pub async fn create_traces_table(table_uri: &str) -> Result<DeltaTable, DeltaTableError> {
    let columns = vec![
        StructField::new("date", DeltaDataType::Primitive(PrimitiveType::Date), false),
        StructField::new(
            "trace_id",
            DeltaDataType::Primitive(PrimitiveType::String),
            false,
        ),
        StructField::new(
            "span_id",
            DeltaDataType::Primitive(PrimitiveType::String),
            false,
        ),
        StructField::new(
            "parent_span_id",
            DeltaDataType::Primitive(PrimitiveType::String),
            true,
        ),
        StructField::new(
            "name",
            DeltaDataType::Primitive(PrimitiveType::String),
            false,
        ),
        StructField::new(
            "kind",
            DeltaDataType::Primitive(PrimitiveType::String),
            false,
        ),
        StructField::new(
            "start_time",
            DeltaDataType::Primitive(PrimitiveType::Timestamp),
            false,
        ),
        StructField::new(
            "end_time",
            DeltaDataType::Primitive(PrimitiveType::Timestamp),
            false,
        ),
        StructField::new(
            "duration_ms",
            DeltaDataType::Primitive(PrimitiveType::Long),
            false,
        ),
        StructField::new(
            "status_code",
            DeltaDataType::Primitive(PrimitiveType::String),
            true,
        ),
        StructField::new(
            "status_message",
            DeltaDataType::Primitive(PrimitiveType::String),
            true,
        ),
        StructField::new(
            "service",
            DeltaDataType::Primitive(PrimitiveType::String),
            false,
        ),
        StructField::new(
            "attributes",
            DeltaDataType::Map(Box::new(MapType::new(
                DeltaDataType::Primitive(PrimitiveType::String),
                DeltaDataType::Primitive(PrimitiveType::String),
                true,
            ))),
            true,
        ),
        StructField::new(
            "events",
            DeltaDataType::Array(Box::new(ArrayType::new(
                DeltaDataType::Struct(Box::new(StructType::new(vec![
                    StructField::new(
                        "timestamp",
                        DeltaDataType::Primitive(PrimitiveType::Timestamp),
                        false,
                    ),
                    StructField::new(
                        "name",
                        DeltaDataType::Primitive(PrimitiveType::String),
                        false,
                    ),
                    StructField::new(
                        "attributes",
                        DeltaDataType::Map(Box::new(MapType::new(
                            DeltaDataType::Primitive(PrimitiveType::String),
                            DeltaDataType::Primitive(PrimitiveType::String),
                            true,
                        ))),
                        true,
                    ),
                ]))),
                true,
            ))),
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

pub fn traces_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("date", DataType::Date32, false),
        Field::new("trace_id", DataType::Utf8, false),
        Field::new("span_id", DataType::Utf8, false),
        Field::new("parent_span_id", DataType::Utf8, true),
        Field::new("name", DataType::Utf8, false),
        Field::new("kind", DataType::Utf8, false),
        Field::new(
            "start_time",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new(
            "end_time",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new("duration_ms", DataType::Int64, false),
        Field::new("status_code", DataType::Utf8, true),
        Field::new("status_message", DataType::Utf8, true),
        Field::new("service", DataType::Utf8, false),
        Field::new(
            "attributes",
            DataType::Map(
                Arc::new(Field::new(
                    "entries",
                    DataType::Struct(
                        vec![
                            Field::new("key", DataType::Utf8, false),
                            Field::new("value", DataType::Utf8, true),
                        ]
                        .into(),
                    ),
                    false,
                )),
                false,
            ),
            true,
        ),
        Field::new(
            "events",
            DataType::List(Arc::new(Field::new(
                "item",
                DataType::Struct(
                    vec![
                        Field::new(
                            "timestamp",
                            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                            false,
                        ),
                        Field::new("name", DataType::Utf8, false),
                        Field::new(
                            "attributes",
                            DataType::Map(
                                Arc::new(Field::new(
                                    "entries",
                                    DataType::Struct(
                                        vec![
                                            Field::new("key", DataType::Utf8, false),
                                            Field::new("value", DataType::Utf8, true),
                                        ]
                                        .into(),
                                    ),
                                    false,
                                )),
                                false,
                            ),
                            true,
                        ),
                    ]
                    .into(),
                ),
                true,
            ))),
            true,
        ),
    ]))
}
