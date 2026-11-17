use fcb_core::ColumnType;
use std::collections::HashMap;

/// Metadata about the FlatCityBuf file, loaded at server startup
#[derive(Clone, Debug)]
pub struct FcbMetadata {
    /// Canonical storage CRS URI, validated at startup.
    pub storage_crs: String,
    pub cityjson: serde_json::Value,
    pub extent: Option<[f64; 6]>,
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
            extent: None,
            cityjson: serde_json::Value::Null,
            storage_crs: String::new(),
        }
    }

    pub fn from_header(header: fcb_core::Header<'_>) -> anyhow::Result<Self> {
        let mut metadata = Self::new();
        metadata.extent = header.geographical_extent().map(|e| {
            [
                e.min().x(),
                e.min().y(),
                e.min().z(),
                e.max().x(),
                e.max().y(),
                e.max().z(),
            ]
        });
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_matches_complete_oracle_documents() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../conformance");
        for name in [
            "header_metadata_full",
            "noise_extension",
            "geom_temp",
            "geometry_instance_interleaved",
            "empty_appearance",
        ] {
            let reader = fcb_core::FcbReader::open(std::io::BufReader::new(
                std::fs::File::open(root.join(format!("{name}.fcb"))).unwrap(),
            ))
            .unwrap();
            let expected =
                std::fs::read_to_string(root.join(format!("{name}.expected.jsonl"))).unwrap();
            assert_eq!(
                FcbMetadata::from_header(reader.header()).unwrap().cityjson,
                serde_json::from_str::<serde_json::Value>(expected.lines().next().unwrap())
                    .unwrap(),
                "{name}"
            );
        }
    }
}
