use opentelemetry_proto::tonic::common::v1::{KeyValue, any_value};
use serde_json::{Map, Value as JsonValue};

use super::bytes_to_hex;

pub fn any_value_to_json(any_val: &any_value::Value) -> JsonValue {
    match any_val {
        any_value::Value::StringValue(s) => JsonValue::String(s.clone()),
        any_value::Value::IntValue(i) => JsonValue::Number((*i).into()),
        any_value::Value::DoubleValue(d) => {
            if let Some(n) = serde_json::Number::from_f64(*d) {
                JsonValue::Number(n)
            } else {
                JsonValue::Null
            }
        }
        any_value::Value::BoolValue(b) => JsonValue::Bool(*b),
        any_value::Value::ArrayValue(a) => {
            let vec: Vec<JsonValue> = a
                .values
                .iter()
                .filter_map(|v| v.value.as_ref().map(any_value_to_json))
                .collect();
            JsonValue::Array(vec)
        }
        any_value::Value::KvlistValue(kv) => {
            let mut map = Map::new();
            for k in &kv.values {
                if let Some(v) = &k.value {
                    if let Some(val) = &v.value {
                        map.insert(k.key.clone(), any_value_to_json(val));
                    }
                }
            }
            JsonValue::Object(map)
        }
        any_value::Value::BytesValue(b) => {
            let encoded: String = bytes_to_hex(b);
            JsonValue::String(encoded)
        }
    }
}

pub fn key_values_to_json(kvs: &[KeyValue]) -> Option<String> {
    if kvs.is_empty() {
        return None;
    }

    let mut map = Map::new();
    for kv in kvs {
        if let Some(v) = &kv.value {
            if let Some(val) = &v.value {
                map.insert(kv.key.clone(), any_value_to_json(val));
            }
        }
    }

    serde_json::to_string(&map).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry_proto::tonic::common::v1::{AnyValue, ArrayValue, KeyValueList};

    #[test]
    fn test_any_value_to_json() {
        assert_eq!(
            any_value_to_json(&any_value::Value::StringValue("hello".to_string())),
            JsonValue::String("hello".to_string())
        );

        assert_eq!(
            any_value_to_json(&any_value::Value::IntValue(42)),
            JsonValue::Number(42.into())
        );

        assert_eq!(
            any_value_to_json(&any_value::Value::DoubleValue(3.14)),
            JsonValue::Number(serde_json::Number::from_f64(3.14).unwrap())
        );

        assert_eq!(
            any_value_to_json(&any_value::Value::DoubleValue(f64::NAN)),
            JsonValue::Null
        );

        assert_eq!(
            any_value_to_json(&any_value::Value::DoubleValue(f64::INFINITY)),
            JsonValue::Null
        );

        assert_eq!(
            any_value_to_json(&any_value::Value::DoubleValue(f64::NEG_INFINITY)),
            JsonValue::Null
        );

        assert_eq!(
            any_value_to_json(&any_value::Value::BoolValue(true)),
            JsonValue::Bool(true)
        );

        assert_eq!(
            any_value_to_json(&any_value::Value::BytesValue(vec![0xde, 0xad, 0xbe, 0xef])),
            JsonValue::String("deadbeef".to_string())
        );

        let array_val = any_value::Value::ArrayValue(ArrayValue {
            values: vec![
                AnyValue {
                    value: Some(any_value::Value::IntValue(1)),
                },
                AnyValue { value: None },
                AnyValue {
                    value: Some(any_value::Value::StringValue("two".to_string())),
                },
            ],
        });
        assert_eq!(
            any_value_to_json(&array_val),
            JsonValue::Array(vec![
                JsonValue::Number(1.into()),
                JsonValue::String("two".to_string()),
            ])
        );

        let kv_val = any_value::Value::KvlistValue(KeyValueList {
            values: vec![
                KeyValue {
                    key: "key1".to_string(),
                    value: Some(AnyValue {
                        value: Some(any_value::Value::BoolValue(false)),
                    }),
                },
                KeyValue {
                    key: "key2".to_string(),
                    value: None,
                },
                KeyValue {
                    key: "key3".to_string(),
                    value: Some(AnyValue { value: None }),
                },
            ],
        });
        let mut expected_map = Map::new();
        expected_map.insert("key1".to_string(), JsonValue::Bool(false));
        assert_eq!(any_value_to_json(&kv_val), JsonValue::Object(expected_map));
    }

    #[test]
    fn test_key_values_to_json() {
        assert_eq!(key_values_to_json(&[]), None);

        let kvs = vec![
            KeyValue {
                key: "k1".to_string(),
                value: Some(AnyValue {
                    value: Some(any_value::Value::StringValue("v1".to_string())),
                }),
            },
            KeyValue {
                key: "k_none1".to_string(),
                value: None,
            },
            KeyValue {
                key: "k_none2".to_string(),
                value: Some(AnyValue { value: None }),
            },
            KeyValue {
                key: "k2".to_string(),
                value: Some(AnyValue {
                    value: Some(any_value::Value::IntValue(42)),
                }),
            },
        ];

        let json_str = key_values_to_json(&kvs).unwrap();
        let parsed: JsonValue = serde_json::from_str(&json_str).unwrap();

        let mut expected_map = Map::new();
        expected_map.insert("k1".to_string(), JsonValue::String("v1".to_string()));
        expected_map.insert("k2".to_string(), JsonValue::Number(42.into()));

        assert_eq!(parsed, JsonValue::Object(expected_map));
    }
}
