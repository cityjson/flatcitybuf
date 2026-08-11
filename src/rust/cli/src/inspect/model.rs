//! Owned, borrow-free snapshot of an FCB header for the inspect TUI and the
//! static report.

use fcb_core::Header;

/// Borrow-free snapshot of an FCB header for rendering.
#[derive(Debug, Clone)]
pub struct InspectModel {
    /// What was inspected: the local path or the URL, as the user typed it.
    pub source: String,
    /// Total size of the file in bytes. `None` for an HTTP source, where the
    /// header read never learns the length of the whole object.
    pub size_bytes: Option<u64>,
    pub title: Option<String>,
    pub identifier: Option<String>,
    pub version: String,
    pub features_count: u64,
    pub reference_date: Option<String>,
    /// R-tree node size; `0` means the writer omitted the spatial index.
    pub index_node_size: u16,
    /// Number of attribute index entries in the header.
    pub attribute_index_count: usize,
    /// Names of the indexed attributes, resolved through the header columns.
    /// An entry whose column is missing cannot be named, so this may be
    /// shorter than [`InspectModel::attribute_index_count`].
    pub attribute_index_names: Vec<String>,
    pub columns: Vec<ColumnInfo>,
    pub crs: Option<CrsInfo>,
    pub extent: Option<ExtentInfo>,
    pub transform: Option<TransformInfo>,
}

impl InspectModel {
    /// True when the header declares a packed Hilbert R-tree. A node size of
    /// zero is how a file written with `--no-spatial-index` records its
    /// absence.
    pub fn has_spatial_index(&self) -> bool {
        self.index_node_size > 0
    }
}

/// Owned snapshot of one FCB column's metadata.
#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub name: String,
    pub type_name: String,
    pub description: Option<String>,
    pub nullable: bool,
    pub primary_key: bool,
    pub unique: bool,
}

/// Owned snapshot of the FCB header's coordinate reference system.
#[derive(Debug, Clone)]
pub struct CrsInfo {
    pub authority: Option<String>,
    pub code: i32,
    pub version: i32,
    pub code_string: Option<String>,
}

impl CrsInfo {
    /// `"EPSG:4326"` when an authority is present, otherwise the bare code.
    pub fn code_label(&self) -> String {
        match &self.authority {
            Some(auth) => format!("{auth}:{}", self.code),
            None => self.code.to_string(),
        }
    }
}

/// Owned snapshot of the FCB header's geographical extent (min/max corners).
#[derive(Debug, Clone)]
pub struct ExtentInfo {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl ExtentInfo {
    /// Width, height, depth = max - min per axis.
    pub fn dimensions(&self) -> [f64; 3] {
        [
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        ]
    }
}

/// Owned snapshot of the FCB header's coordinate scale/translate transform.
#[derive(Debug, Clone)]
pub struct TransformInfo {
    pub scale: [f64; 3],
    pub translate: [f64; 3],
}

/// Build an owned snapshot from a borrowed header. All borrowed `&str`/vector
/// data is copied so the reader (and its buffer) can be dropped afterwards.
///
/// `source` is the path or URL as typed, and `size_bytes` the total file size
/// where it is known (local files only).
pub fn from_header(header: &Header, source: &str, size_bytes: Option<u64>) -> InspectModel {
    let columns = header
        .columns()
        .map(|cols| {
            cols.iter()
                .map(|c| ColumnInfo {
                    name: c.name().to_string(),
                    type_name: c.type_().variant_name().unwrap_or("Unknown").to_string(),
                    description: c.description().map(|s| s.to_string()),
                    nullable: c.nullable(),
                    primary_key: c.primary_key(),
                    unique: c.unique(),
                })
                .collect()
        })
        .unwrap_or_default();

    let crs = header.reference_system().map(|rs| CrsInfo {
        authority: rs.authority().map(|s| s.to_string()),
        code: rs.code(),
        version: rs.version(),
        code_string: rs.code_string().map(|s| s.to_string()),
    });

    let extent = header.geographical_extent().map(|e| ExtentInfo {
        min: [e.min().x(), e.min().y(), e.min().z()],
        max: [e.max().x(), e.max().y(), e.max().z()],
    });

    let transform = header.transform().map(|t| TransformInfo {
        scale: [t.scale().x(), t.scale().y(), t.scale().z()],
        translate: [t.translate().x(), t.translate().y(), t.translate().z()],
    });

    // Each attribute index entry points at a column by index; resolve those to
    // names so the report can list what is queryable, not just how many.
    let attribute_index_names = header
        .attribute_index()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|ai| {
                    header
                        .columns()
                        .iter()
                        .flat_map(|cols| cols.iter())
                        .find(|col| col.index() == ai.index())
                        .map(|col| col.name().to_string())
                })
                .collect()
        })
        .unwrap_or_default();

    InspectModel {
        source: source.to_string(),
        size_bytes,
        title: header.title().map(|s| s.to_string()),
        identifier: header.identifier().map(|s| s.to_string()),
        version: header.version().to_string(),
        features_count: header.features_count(),
        reference_date: header.reference_date().map(|s| s.to_string()),
        index_node_size: header.index_node_size(),
        attribute_index_count: header.attribute_index().map(|v| v.len()).unwrap_or(0),
        attribute_index_names,
        columns,
        crs,
        extent,
        transform,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fcb_core::FcbReader;
    use std::fs::File;
    use std::io::BufReader;
    use std::path::PathBuf;

    fn corpus(name: &str) -> PathBuf {
        // <workspace>/conformance/<name>. cli crate is at src/rust/cli.
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../conformance")
            .join(name)
    }

    #[test]
    fn builds_model_from_header() {
        let path = corpus("inferable_types.fcb");
        let reader = BufReader::new(File::open(&path).expect("open fixture"));
        let fcb = FcbReader::open(reader).expect("open fcb");
        let model = from_header(&fcb.header(), "inferable_types.fcb", Some(1234));

        // Every FCB header carries a version string.
        assert!(!model.version.is_empty());
        // The fixture has attribute columns; ensure we captured names + types.
        assert!(!model.columns.is_empty());
        for col in &model.columns {
            assert!(!col.name.is_empty());
            assert!(!col.type_name.is_empty());
        }
        assert_eq!(model.source, "inferable_types.fcb");
        assert_eq!(model.size_bytes, Some(1234));
        // Every indexed attribute that resolves to a column is named, so the
        // name list can never be longer than the entry count.
        assert!(model.attribute_index_names.len() <= model.attribute_index_count);
    }

    #[test]
    fn spatial_index_presence_follows_node_size() {
        let path = corpus("inferable_types.fcb");
        let reader = BufReader::new(File::open(&path).expect("open fixture"));
        let fcb = FcbReader::open(reader).expect("open fcb");
        let mut model = from_header(&fcb.header(), "x.fcb", None);

        model.index_node_size = 16;
        assert!(model.has_spatial_index());
        model.index_node_size = 0;
        assert!(!model.has_spatial_index());
    }

    #[test]
    fn crs_code_label_formats_authority_and_code() {
        let crs = CrsInfo {
            authority: Some("EPSG".to_string()),
            code: 4326,
            version: 0,
            code_string: None,
        };
        assert_eq!(crs.code_label(), "EPSG:4326");

        let crs_no_auth = CrsInfo {
            authority: None,
            code: 28992,
            version: 0,
            code_string: None,
        };
        assert_eq!(crs_no_auth.code_label(), "28992");
    }

    #[test]
    fn extent_dimensions_are_max_minus_min() {
        let e = ExtentInfo {
            min: [0.0, 0.0, 0.0],
            max: [10.0, 20.0, 5.0],
        };
        assert_eq!(e.dimensions(), [10.0, 20.0, 5.0]);
    }
}
