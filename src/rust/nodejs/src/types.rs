use chrono::{DateTime, Utc};
use fcb_core::static_btree::{FixedStringKey, Float, KeyType, Operator};
use napi::bindgen_prelude::*;

/// Parse operator string into fcb_core Operator
pub fn parse_operator(op: &str) -> Result<Operator> {
    match op {
        "Eq" => Ok(Operator::Eq),
        "Gt" => Ok(Operator::Gt),
        "Ge" => Ok(Operator::Ge),
        "Lt" => Ok(Operator::Lt),
        "Le" => Ok(Operator::Le),
        "Ne" => Ok(Operator::Ne),
        _ => Err(Error::from_reason(format!("Invalid operator: {op}"))),
    }
}

/// Parse a JS value into an fcb_core KeyType for attribute queries.
///
/// JS types map as follows:
/// - boolean → KeyType::Bool
/// - string (ISO 8601 date) → KeyType::DateTime (if parseable)
/// - string → KeyType::StringKey50 or StringKey100
/// - number → KeyType::Float64
pub fn js_value_to_keytype(value: &serde_json::Value) -> Result<KeyType> {
    match value {
        serde_json::Value::Bool(b) => Ok(KeyType::Bool(*b)),
        serde_json::Value::Number(n) => {
            let f = n
                .as_f64()
                .ok_or_else(|| Error::from_reason("Number must be a finite f64"))?;
            Ok(KeyType::Float64(Float(f)))
        }
        serde_json::Value::String(s) => {
            // Try to parse as DateTime first
            if let Ok(dt) = s.parse::<DateTime<Utc>>() {
                return Ok(KeyType::DateTime(dt));
            }
            if s.len() > 50 {
                Ok(KeyType::StringKey100(FixedStringKey::<100>::from_str(s)))
            } else {
                Ok(KeyType::StringKey50(FixedStringKey::<50>::from_str(s)))
            }
        }
        _ => Err(Error::from_reason(format!(
            "Unsupported value type in query: {value}"
        ))),
    }
}
