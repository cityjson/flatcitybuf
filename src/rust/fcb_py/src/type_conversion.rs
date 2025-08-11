use crate::error::FcbError;
use fcb_core::{fb::ColumnType, Header, KeyType};
use ordered_float::OrderedFloat;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyFloat, PyInt, PyString};

/// Convert a Python object to KeyType based on column metadata
pub fn python_value_to_keytype(
    py: Python,
    value: &PyObject,
    field_name: &str,
    header: &Header,
) -> PyResult<KeyType> {
    // First, try to get column type from header metadata
    let column_type = get_column_type(header, field_name);

    let any_value = value.as_ref(py);

    // Try to extract the Python value based on type
    if let Ok(s) = any_value.downcast::<PyString>() {
        let string_value = s.to_str()?;
        convert_string_to_keytype(string_value, column_type)
    } else if let Ok(i) = any_value.downcast::<PyInt>() {
        let int_value = i.extract::<i64>()?;
        convert_int_to_keytype(int_value, column_type)
    } else if let Ok(f) = any_value.downcast::<PyFloat>() {
        let float_value = f.extract::<f64>()?;
        convert_float_to_keytype(float_value, column_type)
    } else if let Ok(b) = any_value.downcast::<PyBool>() {
        let bool_value = b.extract::<bool>()?;
        Ok(KeyType::Bool(bool_value))
    } else {
        // Fall back to string representation
        let string_repr = any_value.str()?.to_str()?;
        convert_string_to_keytype(string_repr, column_type)
    }
}

/// Get column type from header metadata
fn get_column_type(header: &Header, field_name: &str) -> Option<ColumnType> {
    header.columns()?.iter().find_map(|col| {
        if col.name() == field_name {
            Some(col.type_())
        } else {
            None
        }
    })
}

/// Convert string value to appropriate KeyType
fn convert_string_to_keytype(
    value_str: &str,
    column_type: Option<ColumnType>,
) -> PyResult<KeyType> {
    // First try to parse as different types based on content
    let parsed_value = parse_string_value(value_str)?;

    // If we have column metadata, try to convert to match column type
    if let Some(col_type) = column_type {
        match (col_type, &parsed_value) {
            // Integer to Float conversions
            (ColumnType::Float, KeyType::Int32(i)) => {
                Ok(KeyType::Float32(OrderedFloat::from(*i as f32)))
            }
            (ColumnType::Double, KeyType::Int32(i)) => {
                Ok(KeyType::Float64(OrderedFloat::from(*i as f64)))
            }
            (ColumnType::Float, KeyType::Int64(i)) => {
                Ok(KeyType::Float32(OrderedFloat::from(*i as f32)))
            }
            (ColumnType::Double, KeyType::Int64(i)) => {
                Ok(KeyType::Float64(OrderedFloat::from(*i as f64)))
            }

            // Float precision conversions
            (ColumnType::Float, KeyType::Float64(f)) => {
                Ok(KeyType::Float32(OrderedFloat::from(f.0 as f32)))
            }
            (ColumnType::Double, KeyType::Float32(f)) => {
                Ok(KeyType::Float64(OrderedFloat::from(f.0 as f64)))
            }

            // Integer type conversions with range checks
            (ColumnType::Byte, KeyType::Int32(i)) => {
                if *i >= i8::MIN as i32 && *i <= i8::MAX as i32 {
                    Ok(KeyType::Int8(*i as i8))
                } else {
                    Err(PyErr::new::<FcbError, _>(format!(
                        "Value {} out of range for Byte",
                        i
                    )))
                }
            }
            (ColumnType::UByte, KeyType::Int32(i)) => {
                if *i >= 0 && *i <= u8::MAX as i32 {
                    Ok(KeyType::UInt8(*i as u8))
                } else {
                    Err(PyErr::new::<FcbError, _>(format!(
                        "Value {} out of range for UByte",
                        i
                    )))
                }
            }
            (ColumnType::Short, KeyType::Int32(i)) => {
                if *i >= i16::MIN as i32 && *i <= i16::MAX as i32 {
                    Ok(KeyType::Int16(*i as i16))
                } else {
                    Err(PyErr::new::<FcbError, _>(format!(
                        "Value {} out of range for Short",
                        i
                    )))
                }
            }
            (ColumnType::UShort, KeyType::Int32(i)) => {
                if *i >= 0 && *i <= u16::MAX as i32 {
                    Ok(KeyType::UInt16(*i as u16))
                } else {
                    Err(PyErr::new::<FcbError, _>(format!(
                        "Value {} out of range for UShort",
                        i
                    )))
                }
            }
            (ColumnType::Long, KeyType::Int32(i)) => Ok(KeyType::Int64(*i as i64)),
            (ColumnType::ULong, KeyType::Int32(i)) => {
                if *i >= 0 {
                    Ok(KeyType::UInt64(*i as u64))
                } else {
                    Err(PyErr::new::<FcbError, _>(format!(
                        "Value {} cannot be ULong",
                        i
                    )))
                }
            }

            // Larger integer conversions
            (ColumnType::Byte, KeyType::Int64(i)) => {
                if *i >= i8::MIN as i64 && *i <= i8::MAX as i64 {
                    Ok(KeyType::Int8(*i as i8))
                } else {
                    Err(PyErr::new::<FcbError, _>(format!(
                        "Value {} out of range for Byte",
                        i
                    )))
                }
            }
            (ColumnType::UByte, KeyType::Int64(i)) => {
                if *i >= 0 && *i <= u8::MAX as i64 {
                    Ok(KeyType::UInt8(*i as u8))
                } else {
                    Err(PyErr::new::<FcbError, _>(format!(
                        "Value {} out of range for UByte",
                        i
                    )))
                }
            }
            (ColumnType::Short, KeyType::Int64(i)) => {
                if *i >= i16::MIN as i64 && *i <= i16::MAX as i64 {
                    Ok(KeyType::Int16(*i as i16))
                } else {
                    Err(PyErr::new::<FcbError, _>(format!(
                        "Value {} out of range for Short",
                        i
                    )))
                }
            }
            (ColumnType::UShort, KeyType::Int64(i)) => {
                if *i >= 0 && *i <= u16::MAX as i64 {
                    Ok(KeyType::UInt16(*i as u16))
                } else {
                    Err(PyErr::new::<FcbError, _>(format!(
                        "Value {} out of range for UShort",
                        i
                    )))
                }
            }
            (ColumnType::Int, KeyType::Int64(i)) => {
                if *i >= i32::MIN as i64 && *i <= i32::MAX as i64 {
                    Ok(KeyType::Int32(*i as i32))
                } else {
                    Err(PyErr::new::<FcbError, _>(format!(
                        "Value {} out of range for Int",
                        i
                    )))
                }
            }
            (ColumnType::UInt, KeyType::Int64(i)) => {
                if *i >= 0 && *i <= u32::MAX as i64 {
                    Ok(KeyType::UInt32(*i as u32))
                } else {
                    Err(PyErr::new::<FcbError, _>(format!(
                        "Value {} out of range for UInt",
                        i
                    )))
                }
            }
            (ColumnType::ULong, KeyType::Int64(i)) => {
                if *i >= 0 {
                    Ok(KeyType::UInt64(*i as u64))
                } else {
                    Err(PyErr::new::<FcbError, _>(format!(
                        "Value {} cannot be ULong",
                        i
                    )))
                }
            }

            // Type already matches or String type
            _ => Ok(parsed_value),
        }
    } else {
        // No metadata available, return parsed value as-is
        Ok(parsed_value)
    }
}

/// Convert integer value to KeyType based on column type
fn convert_int_to_keytype(value: i64, column_type: Option<ColumnType>) -> PyResult<KeyType> {
    if let Some(col_type) = column_type {
        match col_type {
            ColumnType::Byte => {
                if value >= i8::MIN as i64 && value <= i8::MAX as i64 {
                    Ok(KeyType::Int8(value as i8))
                } else {
                    Err(PyErr::new::<FcbError, _>(format!(
                        "Value {} out of range for Byte",
                        value
                    )))
                }
            }
            ColumnType::UByte => {
                if value >= 0 && value <= u8::MAX as i64 {
                    Ok(KeyType::UInt8(value as u8))
                } else {
                    Err(PyErr::new::<FcbError, _>(format!(
                        "Value {} out of range for UByte",
                        value
                    )))
                }
            }
            ColumnType::Short => {
                if value >= i16::MIN as i64 && value <= i16::MAX as i64 {
                    Ok(KeyType::Int16(value as i16))
                } else {
                    Err(PyErr::new::<FcbError, _>(format!(
                        "Value {} out of range for Short",
                        value
                    )))
                }
            }
            ColumnType::UShort => {
                if value >= 0 && value <= u16::MAX as i64 {
                    Ok(KeyType::UInt16(value as u16))
                } else {
                    Err(PyErr::new::<FcbError, _>(format!(
                        "Value {} out of range for UShort",
                        value
                    )))
                }
            }
            ColumnType::Int => {
                if value >= i32::MIN as i64 && value <= i32::MAX as i64 {
                    Ok(KeyType::Int32(value as i32))
                } else {
                    Err(PyErr::new::<FcbError, _>(format!(
                        "Value {} out of range for Int",
                        value
                    )))
                }
            }
            ColumnType::UInt => {
                if value >= 0 && value <= u32::MAX as i64 {
                    Ok(KeyType::UInt32(value as u32))
                } else {
                    Err(PyErr::new::<FcbError, _>(format!(
                        "Value {} out of range for UInt",
                        value
                    )))
                }
            }
            ColumnType::Long => Ok(KeyType::Int64(value)),
            ColumnType::ULong => {
                if value >= 0 {
                    Ok(KeyType::UInt64(value as u64))
                } else {
                    Err(PyErr::new::<FcbError, _>(format!(
                        "Value {} cannot be ULong",
                        value
                    )))
                }
            }
            ColumnType::Float => Ok(KeyType::Float32(OrderedFloat::from(value as f32))),
            ColumnType::Double => Ok(KeyType::Float64(OrderedFloat::from(value as f64))),
            _ => Ok(KeyType::Int64(value)), // Default to Int64
        }
    } else {
        // No metadata, choose appropriate int type based on value range
        if value >= i32::MIN as i64 && value <= i32::MAX as i64 {
            Ok(KeyType::Int32(value as i32))
        } else {
            Ok(KeyType::Int64(value))
        }
    }
}

/// Convert float value to KeyType based on column type
fn convert_float_to_keytype(value: f64, column_type: Option<ColumnType>) -> PyResult<KeyType> {
    if let Some(col_type) = column_type {
        match col_type {
            ColumnType::Float => Ok(KeyType::Float32(OrderedFloat::from(value as f32))),
            ColumnType::Double => Ok(KeyType::Float64(OrderedFloat::from(value))),
            _ => Ok(KeyType::Float64(OrderedFloat::from(value))), // Default to Float64
        }
    } else {
        Ok(KeyType::Float64(OrderedFloat::from(value)))
    }
}

/// Parse a string value to the most appropriate KeyType
fn parse_string_value(value_str: &str) -> PyResult<KeyType> {
    // Try boolean first
    match value_str.to_lowercase().as_str() {
        "true" | "yes" | "1" => return Ok(KeyType::Bool(true)),
        "false" | "no" | "0" => return Ok(KeyType::Bool(false)),
        _ => {}
    }

    // Try integer
    if let Ok(i) = value_str.parse::<i32>() {
        return Ok(KeyType::Int32(i));
    }

    // Try larger integer
    if let Ok(i) = value_str.parse::<i64>() {
        return Ok(KeyType::Int64(i));
    }

    // Try float
    if let Ok(f) = value_str.parse::<f64>() {
        return Ok(KeyType::Float64(OrderedFloat::from(f)));
    }

    // Default to string
    // FIXME: this is a temporary fix to allow for string values to be converted to KeyType. Also we should consider the way to handle the variable length string key.
    Ok(fcb_core::KeyType::StringKey50(
        fcb_core::FixedStringKey::from_str(value_str),
    ))
}
