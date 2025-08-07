use crate::metadata::FcbMetadata;
use fcb_core::{ColumnType, FixedStringKey, KeyType, Operator};
use ordered_float::OrderedFloat;
use thiserror::Error;

#[derive(Debug, PartialEq, Error)]
pub enum ParseError {
    #[error("Invalid syntax: {0}")]
    InvalidSyntax(String),
    #[error("Unsupported operator: {0}")]
    UnsupportedOperator(String),
    #[error("Invalid value: {0}")]
    InvalidValue(String),
}

pub type FilterCondition = (String, Operator, KeyType);

pub fn parse_filter(
    filter: &str,
    metadata: &FcbMetadata,
) -> Result<Vec<FilterCondition>, ParseError> {
    let mut conditions = Vec::new();

    // Normalize whitespace
    let normalized_filter = filter.split_whitespace().collect::<Vec<_>>().join(" ");

    // First, extract all BETWEEN expressions and replace them with placeholders
    let mut working_filter = normalized_filter.clone();
    let mut between_conditions = Vec::new();
    let mut placeholder_counter = 0;

    // Find BETWEEN expressions
    while let Some(between_match) = find_between_expression(&working_filter) {
        let (full_match, attribute, lower_val, upper_val) = between_match;

        // Parse and convert values based on column type
        let lower_value = parse_and_convert_value(&lower_val, &attribute, metadata)?;
        let upper_value = parse_and_convert_value(&upper_val, &attribute, metadata)?;

        // Store the conditions
        between_conditions.push((attribute.clone(), Operator::Ge, lower_value));
        between_conditions.push((attribute, Operator::Le, upper_value));

        // Replace with placeholder
        let placeholder = format!("__BETWEEN_PLACEHOLDER_{placeholder_counter}__");
        working_filter = working_filter.replace(&full_match, &placeholder);
        placeholder_counter += 1;
    }

    // Now split by AND and process remaining conditions
    let parts: Vec<&str> = working_filter
        .split(" AND ")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !s.starts_with("__BETWEEN_PLACEHOLDER_"))
        .collect();

    for part in parts {
        let condition = parse_comparison(part, metadata)?;
        conditions.push(condition);
    }

    // Add all BETWEEN conditions
    conditions.extend(between_conditions);

    Ok(conditions)
}

fn find_between_expression(filter: &str) -> Option<(String, String, String, String)> {
    // Find pattern: "attribute BETWEEN value1 AND value2"
    if let Some(between_start) = filter.find(" BETWEEN ") {
        // Find attribute name (work backwards from BETWEEN)
        let before_between = &filter[..between_start];
        let attr_start = before_between
            .rfind(" AND ")
            .map(|pos| pos + 5)
            .unwrap_or(0);
        let attribute = before_between[attr_start..].trim();

        if !attribute.is_empty() {
            // Find the values after BETWEEN
            let after_between = &filter[between_start + 9..]; // " BETWEEN ".len() = 9

            if let Some(and_pos) = after_between.find(" AND ") {
                let lower_val = after_between[..and_pos].trim();

                // Find end of upper value (next " AND " or end of string)
                let after_and = &after_between[and_pos + 5..]; // " AND ".len() = 5
                let mut end_pos = after_and.len();

                // Look for word boundary to avoid matching partial words
                if let Some(next_and) = after_and.find(" AND ") {
                    // Check if this AND is part of another condition by looking at surrounding context
                    let potential_upper = after_and[..next_and].trim();
                    // Simple heuristic: if the potential upper value doesn't contain operators, use it
                    if !potential_upper.contains('>')
                        && !potential_upper.contains('<')
                        && !potential_upper.contains('=')
                    {
                        end_pos = next_and;
                    }
                }

                let upper_val = after_and[..end_pos].trim();

                if !lower_val.is_empty() && !upper_val.is_empty() {
                    let full_match = format!("{attribute} BETWEEN {lower_val} AND {upper_val}");
                    return Some((
                        full_match,
                        attribute.to_string(),
                        lower_val.to_string(),
                        upper_val.to_string(),
                    ));
                }
            }
        }
    }
    None
}

fn parse_comparison(expr: &str, metadata: &FcbMetadata) -> Result<FilterCondition, ParseError> {
    // Try different operators in order of length (to avoid matching < when we have <=)
    let operators = [
        (">=", Operator::Ge),
        ("<=", Operator::Le),
        ("!=", Operator::Ne),
        ("<>", Operator::Ne),
        ("=", Operator::Eq),
        (">", Operator::Gt),
        ("<", Operator::Lt),
    ];

    for (op_str, op) in operators.iter() {
        if let Some(idx) = expr.find(op_str) {
            // Check that we haven't found a partial match (e.g., > in >>)
            let before_op = &expr[..idx];
            let after_op = &expr[idx + op_str.len()..];

            // Make sure there's no operator character immediately after our found operator
            if let Some(first_char) = after_op.chars().next() {
                if first_char == '>' || first_char == '<' || first_char == '=' || first_char == '!'
                {
                    continue; // This is part of a longer operator, skip
                }
            }

            let attribute = before_op.trim();
            let value_str = after_op.trim();

            if attribute.is_empty() || value_str.is_empty() {
                continue; // Invalid structure, try next operator
            }

            let value = parse_and_convert_value(value_str, attribute, metadata)?;

            return Ok((attribute.to_string(), *op, value));
        }
    }

    Err(ParseError::InvalidSyntax(
        "No valid operator found".to_string(),
    ))
}

fn parse_value(value_str: &str) -> Result<KeyType, ParseError> {
    // Remove quotes if present
    let value_str = if (value_str.starts_with('\'') && value_str.ends_with('\''))
        || (value_str.starts_with('"') && value_str.ends_with('"'))
    {
        &value_str[1..value_str.len() - 1]
    } else {
        value_str
    };

    // Try to parse as different types
    if let Ok(int_val) = value_str.parse::<i32>() {
        Ok(KeyType::Int32(int_val))
    } else if let Ok(int_val) = value_str.parse::<i64>() {
        Ok(KeyType::Int64(int_val))
    } else if let Ok(float_val) = value_str.parse::<f64>() {
        Ok(KeyType::Float64(OrderedFloat(float_val)))
    } else if value_str.eq_ignore_ascii_case("true") {
        Ok(KeyType::Bool(true))
    } else if value_str.eq_ignore_ascii_case("false") {
        Ok(KeyType::Bool(false))
    } else {
        // Default to string
        if value_str.len() <= 20 {
            Ok(KeyType::StringKey20(FixedStringKey::from_str(value_str)))
        } else if value_str.len() <= 50 {
            Ok(KeyType::StringKey50(FixedStringKey::from_str(value_str)))
        } else if value_str.len() <= 100 {
            Ok(KeyType::StringKey100(FixedStringKey::from_str(value_str)))
        } else {
            Err(ParseError::InvalidValue(format!(
                "String value too long: {} characters",
                value_str.len()
            )))
        }
    }
}

fn parse_and_convert_value(
    value_str: &str,
    attribute: &str,
    metadata: &FcbMetadata,
) -> Result<KeyType, ParseError> {
    // First parse the value as-is
    let parsed_value = parse_value(value_str)?;

    // Get the column type from metadata
    let column_type = metadata.get_column_type(attribute);

    // If we don't have metadata for this column, return the parsed value as-is
    let Some(col_type) = column_type else {
        return Ok(parsed_value);
    };

    // Convert the parsed value to match the column type
    match (col_type, &parsed_value) {
        // Integer to Float conversions
        (ColumnType::Float, KeyType::Int32(i)) => {
            Ok(KeyType::Float32(OrderedFloat::from(*i as f32)))
        }
        (ColumnType::Double, KeyType::Int32(i)) => {
            Ok(KeyType::Float64(OrderedFloat::from(*i as f64)))
        }

        // Float to Double conversion
        (ColumnType::Float, KeyType::Float64(f)) => {
            Ok(KeyType::Float32(OrderedFloat::from(f.0 as f32)))
        }
        (ColumnType::Double, KeyType::Float32(f)) => {
            Ok(KeyType::Float64(OrderedFloat::from(f.0 as f64)))
        }

        // Integer type conversions
        (ColumnType::Byte, KeyType::Int32(i)) => {
            if *i >= i8::MIN as i32 && *i <= i8::MAX as i32 {
                Ok(KeyType::Int8(*i as i8))
            } else {
                Err(ParseError::InvalidValue(format!(
                    "Value {i} out of range for Byte"
                )))
            }
        }
        (ColumnType::UByte, KeyType::Int32(i)) => {
            if *i >= 0 && *i <= u8::MAX as i32 {
                Ok(KeyType::UInt8(*i as u8))
            } else {
                Err(ParseError::InvalidValue(format!(
                    "Value {i} out of range for UByte"
                )))
            }
        }
        (ColumnType::Short, KeyType::Int32(i)) => {
            if *i >= i16::MIN as i32 && *i <= i16::MAX as i32 {
                Ok(KeyType::Int16(*i as i16))
            } else {
                Err(ParseError::InvalidValue(format!(
                    "Value {i} out of range for Short"
                )))
            }
        }
        (ColumnType::UShort, KeyType::Int32(i)) => {
            if *i >= 0 && *i <= u16::MAX as i32 {
                Ok(KeyType::UInt16(*i as u16))
            } else {
                Err(ParseError::InvalidValue(format!(
                    "Value {i} out of range for UShort"
                )))
            }
        }
        (ColumnType::Long, KeyType::Int32(i)) => Ok(KeyType::Int64(*i as i64)),
        (ColumnType::ULong, KeyType::Int32(i)) => {
            if *i >= 0 {
                Ok(KeyType::UInt64(*i as u64))
            } else {
                Err(ParseError::InvalidValue(format!(
                    "Value {i} cannot be ULong"
                )))
            }
        }

        // Type already matches or no conversion needed
        _ => Ok(parsed_value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::ColumnMetadata;

    fn create_test_metadata() -> FcbMetadata {
        let mut metadata = FcbMetadata::new();

        // Add some common columns for testing
        metadata.columns.insert(
            "b3_h_dak_50p".to_string(),
            ColumnMetadata {
                index: 1,
                column_type: ColumnType::Float,
                is_indexed: true,
            },
        );

        metadata.columns.insert(
            "b3_bouwlagen".to_string(),
            ColumnMetadata {
                index: 2,
                column_type: ColumnType::Int,
                is_indexed: true,
            },
        );

        metadata.columns.insert(
            "identificatie".to_string(),
            ColumnMetadata {
                index: 3,
                column_type: ColumnType::String,
                is_indexed: true,
            },
        );

        metadata.columns.insert(
            "b3_is_glas_dak".to_string(),
            ColumnMetadata {
                index: 4,
                column_type: ColumnType::Bool,
                is_indexed: true,
            },
        );

        metadata
    }

    #[test]
    fn test_simple_equality() {
        let metadata = create_test_metadata();
        let filter = "identificatie = 'NL.IMBAG.Pand.123'";
        let result = parse_filter(filter, &metadata).unwrap();

        assert_eq!(result.len(), 1);
        let (attr, op, _) = &result[0];
        assert_eq!(attr, "identificatie");
        assert!(matches!(op, Operator::Eq));
    }

    #[test]
    fn test_numeric_comparison() {
        let metadata = create_test_metadata();
        let filter = "building_height > 30";
        let result = parse_filter(filter, &metadata).unwrap();

        assert_eq!(result.len(), 1);
        let (attr, op, key) = &result[0];
        assert_eq!(attr, "building_height");
        assert!(matches!(op, Operator::Gt));
        assert!(matches!(key, KeyType::Int32(30)));
    }

    #[test]
    fn test_float_comparison() {
        let metadata = create_test_metadata();
        let filter = "b3_h_dak_50p >= 10.5";
        let result = parse_filter(filter, &metadata).unwrap();

        assert_eq!(result.len(), 1);
        let (attr, op, key) = &result[0];
        assert_eq!(attr, "b3_h_dak_50p");
        assert!(matches!(op, Operator::Ge));
        // Should be converted to Float32 since b3_h_dak_50p is defined as Float column type
        assert!(matches!(key, KeyType::Float32(_)));
        if let KeyType::Float32(f) = key {
            assert_eq!(f.0, 10.5);
        }
    }

    #[test]
    fn test_and_condition() {
        let metadata = create_test_metadata();
        let filter = "building_height > 30 AND cityname = 'Amsterdam'";
        let result = parse_filter(filter, &metadata).unwrap();

        assert_eq!(result.len(), 2);

        let (attr1, op1, key1) = &result[0];
        assert_eq!(attr1, "building_height");
        assert!(matches!(op1, Operator::Gt));
        assert!(matches!(key1, KeyType::Int32(30)));

        let (attr2, op2, _) = &result[1];
        assert_eq!(attr2, "cityname");
        assert!(matches!(op2, Operator::Eq));
    }

    #[test]
    fn test_between_condition() {
        let metadata = create_test_metadata();
        let filter = "building_height BETWEEN 10 AND 50";
        let result = parse_filter(filter, &metadata).unwrap();

        assert_eq!(result.len(), 2);

        let (attr1, op1, key1) = &result[0];
        assert_eq!(attr1, "building_height");
        assert!(matches!(op1, Operator::Ge));
        assert!(matches!(key1, KeyType::Int32(10)));

        let (attr2, op2, key2) = &result[1];
        assert_eq!(attr2, "building_height");
        assert!(matches!(op2, Operator::Le));
        assert!(matches!(key2, KeyType::Int32(50)));
    }

    #[test]
    fn test_boolean_value() {
        let metadata = create_test_metadata();
        let filter = "b3_is_glas_dak = true";
        let result = parse_filter(filter, &metadata).unwrap();

        assert_eq!(result.len(), 1);
        let (attr, op, key) = &result[0];
        assert_eq!(attr, "b3_is_glas_dak");
        assert!(matches!(op, Operator::Eq));
        assert!(matches!(key, KeyType::Bool(true)));
    }

    #[test]
    fn test_not_equal_operators() {
        let metadata = create_test_metadata();
        let filter1 = "status != 'active'";
        let result1 = parse_filter(filter1, &metadata).unwrap();
        assert!(matches!(result1[0].1, Operator::Ne));

        let filter2 = "status <> 'active'";
        let result2 = parse_filter(filter2, &metadata).unwrap();
        assert!(matches!(result2[0].1, Operator::Ne));
    }

    #[test]
    fn test_complex_query() {
        let metadata = create_test_metadata();
        let filter = "building_height > 30 AND cityname = 'Amsterdam' AND b3_is_glas_dak = true";
        let result = parse_filter(filter, &metadata).unwrap();

        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_invalid_syntax() {
        let metadata = create_test_metadata();
        let filter = "building_height >> 30";
        let result = parse_filter(filter, &metadata);

        if let Ok(ref r) = result {
            println!("Unexpected success: {r:?}");
        }

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ParseError::InvalidSyntax(_)));
    }

    #[test]
    fn test_string_too_long() {
        let metadata = create_test_metadata();
        let long_string = "a".repeat(101);
        let filter = format!("field = '{long_string}'");
        let result = parse_filter(&filter, &metadata);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ParseError::InvalidValue(_)));
    }

    #[test]
    fn test_type_conversion() {
        let metadata = create_test_metadata();

        // Test integer to float conversion for b3_h_dak_50p (Float type)
        let filter = "b3_h_dak_50p > 30";
        let result = parse_filter(filter, &metadata).unwrap();
        assert_eq!(result.len(), 1);
        let (_, _, key) = &result[0];
        // Should be converted to Float32 since b3_h_dak_50p is defined as Float
        assert!(matches!(key, KeyType::Float32(_)));
        if let KeyType::Float32(f) = key {
            assert_eq!(f.0, 30.0);
        }

        // Test that integer columns remain as integers
        let filter2 = "b3_bouwlagen > 2";
        let result2 = parse_filter(filter2, &metadata).unwrap();
        assert_eq!(result2.len(), 1);
        let (_, _, key2) = &result2[0];
        // Should remain as Int32 since b3_bouwlagen is defined as Int
        assert!(matches!(key2, KeyType::Int32(_)));
    }
}
