use fcb_core::ColumnType;
use std::collections::HashMap;

/// Metadata about the FlatCityBuf file, loaded at server startup
#[derive(Clone, Debug)]
pub struct FcbMetadata {
    /// Map from column name to column metadata
    pub cityjson: serde_json::Value,
    pub columns: HashMap<String, ColumnMetadata>,
}

#[derive(Clone, Debug)]
pub struct ColumnMetadata {
    /// The index of this column
    pub index: u16,
    /// The data type of this column
    pub column_type: ColumnType,
    /// Whether this column is indexed
    pub is_indexed: bool,
}

impl Default for FcbMetadata {
    fn default() -> Self {
        Self::new()
    }
}

impl FcbMetadata {
    pub fn new() -> Self {
        Self {
            columns: HashMap::new(),
            cityjson: serde_json::Value::Null,
        }
    }

    pub fn from_header(header: fcb_core::Header<'_>) -> anyhow::Result<Self> {
        let mut metadata = Self::new();
        metadata.cityjson = serde_json::to_value(fcb_core::deserializer::to_cj_metadata(&header)?)?;
        if let Some(columns) = header.columns() {
            for column in columns.iter() {
                metadata.columns.insert(
                    column.name().to_string(),
                    ColumnMetadata {
                        index: column.index(),
                        column_type: column.type_(),
                        is_indexed: header.attribute_index().is_some_and(|indices| {
                            indices.iter().any(|i| i.index() == column.index())
                        }),
                    },
                );
            }
        }
        Ok(metadata)
    }

    /// Get the column type for a given column name
    pub fn get_column_type(&self, column_name: &str) -> Option<ColumnType> {
        self.columns.get(column_name).map(|c| c.column_type)
    }

    /// Check if a column is indexed
    pub fn is_column_indexed(&self, column_name: &str) -> bool {
        self.columns.get(column_name).is_some_and(|c| c.is_indexed)
    }
}
