//! Plain-text rendering of an [`InspectModel`], for non-interactive output.

use std::fmt::Write as _;

use crate::inspect::model::InspectModel;

/// How many indexed attribute names the report lists before summarising the
/// rest as "... N more attributes...".
const MAX_LISTED_ATTRIBUTE_INDICES: usize = 10;

/// Render the full static report. Plain text, no ANSI: the report must be
/// byte-identical whether it was forced with `--static` in a terminal or
/// produced by the non-TTY fallback into a pipe.
///
/// ```
/// # use fcb_cli::inspect::{model::InspectModel, static_report::render};
/// # let model = InspectModel {
/// #     source: "city.fcb".into(), size_bytes: Some(1024), title: None,
/// #     identifier: None, version: "2.0".into(), features_count: 2,
/// #     reference_date: None, index_node_size: 16, attribute_index_count: 0,
/// #     attribute_index_names: Vec::new(), columns: Vec::new(), crs: None,
/// #     extent: None, transform: None,
/// # };
/// assert!(render(&model).contains("▶ File Details"));
/// ```
pub fn render(model: &InspectModel) -> String {
    // `write!` to a String is infallible, so the `let _ =` discards an error
    // that cannot happen rather than hiding one that can.
    let mut out = String::new();

    let _ = writeln!(out, "▶ File Details");
    let _ = writeln!(out, "  Source: {}", model.source);
    let _ = writeln!(
        out,
        "  Size: {}",
        model
            .size_bytes
            .map(format_size)
            // An HTTP header read never learns the length of the whole object,
            // and guessing one would be worse than saying so.
            .unwrap_or_else(|| "unknown (remote)".to_string())
    );
    let _ = writeln!(out, "  Version: {}", model.version);
    if let Some(title) = &model.title {
        let _ = writeln!(out, "  Title: {title}");
    }
    if let Some(identifier) = &model.identifier {
        let _ = writeln!(out, "  Identifier: {identifier}");
    }
    if let Some(date) = &model.reference_date {
        let _ = writeln!(out, "  Reference Date: {date}");
    }

    let _ = writeln!(out, "\n▶ Dataset");
    let _ = writeln!(out, "  Features: {}", model.features_count);
    let _ = writeln!(out, "  Columns: {}", model.columns.len());
    match &model.extent {
        Some(extent) => {
            let _ = writeln!(out, "  Geospatial Extent: Yes");
            let _ = writeln!(
                out,
                "    Min: [{:.2}, {:.2}, {:.2}]",
                extent.min[0], extent.min[1], extent.min[2]
            );
            let _ = writeln!(
                out,
                "    Max: [{:.2}, {:.2}, {:.2}]",
                extent.max[0], extent.max[1], extent.max[2]
            );
            let [width, height, depth] = extent.dimensions();
            let _ = writeln!(out, "    Dimensions: {width:.2} × {height:.2} × {depth:.2}");
        }
        None => {
            let _ = writeln!(out, "  Geospatial Extent: Not set");
        }
    }

    let _ = writeln!(out, "\n▶ Indices");
    if model.has_spatial_index() {
        let _ = writeln!(
            out,
            "  Spatial R-tree: Yes (node size: {})",
            model.index_node_size
        );
    } else {
        let _ = writeln!(out, "  Spatial R-tree: No");
    }
    if model.attribute_index_count == 0 {
        let _ = writeln!(out, "  Attribute Indices: None");
    } else {
        let _ = writeln!(
            out,
            "  Attribute Indices: {} (B+Tree)",
            model.attribute_index_count
        );
        for (i, name) in model
            .attribute_index_names
            .iter()
            .enumerate()
            .take(MAX_LISTED_ATTRIBUTE_INDICES)
        {
            let _ = writeln!(out, "    {}. {name}", i + 1);
        }
        let hidden = model
            .attribute_index_names
            .len()
            .saturating_sub(MAX_LISTED_ATTRIBUTE_INDICES);
        if hidden > 0 {
            let _ = writeln!(out, "    ... {hidden} more attributes...");
        }
    }

    let _ = writeln!(out, "\n▶ Coordinate Reference System");
    match &model.crs {
        Some(crs) => {
            let _ = writeln!(out, "  CRS: {}", crs.code_label());
            // Version 0 is the "unset" encoding, not a real CRS revision.
            if crs.version != 0 {
                let _ = writeln!(out, "  CRS Version: {}", crs.version);
            }
            if let Some(code_string) = &crs.code_string {
                let _ = writeln!(out, "  CRS Code String: {code_string}");
            }
        }
        None => {
            let _ = writeln!(out, "  CRS: Not set");
        }
    }

    let _ = writeln!(out, "\n▶ Coordinate Transform");
    match &model.transform {
        Some(transform) => {
            let _ = writeln!(
                out,
                "  Scale: [{:.6}, {:.6}, {:.6}]",
                transform.scale[0], transform.scale[1], transform.scale[2]
            );
            let _ = writeln!(
                out,
                "  Translate: [{:.6}, {:.6}, {:.6}]",
                transform.translate[0], transform.translate[1], transform.translate[2]
            );
        }
        None => {
            let _ = writeln!(out, "  Transform: Not set");
        }
    }

    out
}

/// Human-readable byte count: bytes below 1 KiB, then two decimals of KB / MB /
/// GB (binary multiples, as the retired `info` command printed them).
pub(crate) fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let size = bytes as f64;
    if size >= GB {
        format!("{:.2} GB", size / GB)
    } else if size >= MB {
        format!("{:.2} MB", size / MB)
    } else if size >= KB {
        format!("{:.2} KB", size / KB)
    } else {
        format!("{bytes} bytes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::model::{ColumnInfo, CrsInfo, ExtentInfo, InspectModel, TransformInfo};

    /// Minimal model: no extent, no transform, no CRS, no indices.
    fn bare_model() -> InspectModel {
        InspectModel {
            source: "city.fcb".into(),
            size_bytes: Some(2048),
            title: None,
            identifier: None,
            version: "2.0".into(),
            features_count: 3,
            reference_date: None,
            index_node_size: 0,
            attribute_index_count: 0,
            attribute_index_names: Vec::new(),
            columns: Vec::new(),
            crs: None,
            extent: None,
            transform: None,
        }
    }

    /// Fully populated model: every optional field present.
    fn full_model() -> InspectModel {
        InspectModel {
            source: "/data/delft.fcb".into(),
            size_bytes: Some(7_662_899),
            title: Some("3DBAG".into()),
            identifier: Some("delft-001".into()),
            version: "2.0".into(),
            features_count: 1115,
            reference_date: Some("2024-01-31".into()),
            index_node_size: 16,
            attribute_index_count: 2,
            attribute_index_names: vec!["b3_dak_type".into(), "identificatie".into()],
            columns: vec![ColumnInfo {
                name: "b3_dak_type".into(),
                type_name: "String".into(),
                description: None,
                nullable: true,
                primary_key: false,
                unique: false,
            }],
            crs: Some(CrsInfo {
                authority: Some("EPSG".into()),
                code: 7415,
                version: 2,
                code_string: Some("EPSG:7415".into()),
            }),
            extent: Some(ExtentInfo {
                min: [84501.55, 445805.03, -3.75],
                max: [85675.23, 446983.47, 95.04],
            }),
            transform: Some(TransformInfo {
                scale: [0.001, 0.001, 0.001],
                translate: [85088.390625, 446394.25, 45.648003],
            }),
        }
    }

    #[test]
    fn file_details_section_lists_source_size_and_version() {
        let out = render(&full_model());
        assert!(out.contains("▶ File Details"));
        assert!(out.contains("  Source: /data/delft.fcb"));
        assert!(out.contains("  Size: 7.31 MB"));
        assert!(out.contains("  Version: 2.0"));
        assert!(out.contains("  Title: 3DBAG"));
        assert!(out.contains("  Identifier: delft-001"));
        assert!(out.contains("  Reference Date: 2024-01-31"));
    }

    #[test]
    fn optional_file_details_are_omitted_when_absent() {
        let out = render(&bare_model());
        assert!(!out.contains("Title:"));
        assert!(!out.contains("Identifier:"));
        assert!(!out.contains("Reference Date:"));
    }

    #[test]
    fn unknown_size_is_reported_rather_than_guessed() {
        let mut model = full_model();
        model.size_bytes = None;
        model.source = "https://example.com/city.fcb".into();
        let out = render(&model);
        assert!(out.contains("  Source: https://example.com/city.fcb"));
        assert!(out.contains("  Size: unknown (remote)"));
    }

    #[test]
    fn dataset_section_shows_features_columns_and_extent() {
        let out = render(&full_model());
        assert!(out.contains("▶ Dataset"));
        assert!(out.contains("  Features: 1115"));
        assert!(out.contains("  Columns: 1"));
        assert!(out.contains("  Geospatial Extent: Yes"));
        assert!(out.contains("    Min: [84501.55, 445805.03, -3.75]"));
        assert!(out.contains("    Max: [85675.23, 446983.47, 95.04]"));
        assert!(out.contains("    Dimensions: 1173.68 × 1178.44 × 98.79"));
    }

    #[test]
    fn absent_extent_is_reported_as_not_set() {
        let out = render(&bare_model());
        assert!(out.contains("  Geospatial Extent: Not set"));
        assert!(!out.contains("Dimensions:"));
    }

    #[test]
    fn indices_section_names_indexed_attributes() {
        let out = render(&full_model());
        assert!(out.contains("▶ Indices"));
        assert!(out.contains("  Spatial R-tree: Yes (node size: 16)"));
        assert!(out.contains("  Attribute Indices: 2 (B+Tree)"));
        assert!(out.contains("    1. b3_dak_type"));
        assert!(out.contains("    2. identificatie"));
    }

    #[test]
    fn absent_indices_are_reported_as_no_and_none() {
        let out = render(&bare_model());
        assert!(out.contains("  Spatial R-tree: No"));
        assert!(!out.contains("node size"));
        assert!(out.contains("  Attribute Indices: None"));
    }

    #[test]
    fn long_attribute_index_lists_are_truncated_after_ten() {
        let mut model = full_model();
        model.attribute_index_names = (0..12).map(|i| format!("attr_{i:02}")).collect();
        model.attribute_index_count = model.attribute_index_names.len();
        let out = render(&model);
        assert!(out.contains("    10. attr_09"));
        assert!(!out.contains("attr_10"));
        assert!(out.contains("    ... 2 more attributes..."));
    }

    #[test]
    fn crs_section_shows_code_version_and_code_string() {
        let out = render(&full_model());
        assert!(out.contains("▶ Coordinate Reference System"));
        assert!(out.contains("  CRS: EPSG:7415"));
        assert!(out.contains("  CRS Version: 2"));
        assert!(out.contains("  CRS Code String: EPSG:7415"));
    }

    #[test]
    fn absent_crs_is_reported_as_not_set() {
        let out = render(&bare_model());
        assert!(out.contains("▶ Coordinate Reference System"));
        assert!(out.contains("  CRS: Not set"));
    }

    #[test]
    fn crs_version_zero_is_omitted() {
        let mut model = full_model();
        if let Some(crs) = model.crs.as_mut() {
            crs.version = 0;
        }
        let out = render(&model);
        assert!(out.contains("  CRS: EPSG:7415"));
        assert!(!out.contains("  CRS Version:"));
    }

    #[test]
    fn transform_section_shows_scale_and_translate() {
        let out = render(&full_model());
        assert!(out.contains("▶ Coordinate Transform"));
        assert!(out.contains("  Scale: [0.001000, 0.001000, 0.001000]"));
        assert!(out.contains("  Translate: [85088.390625, 446394.250000, 45.648003]"));
    }

    #[test]
    fn absent_transform_is_reported_as_not_set() {
        let out = render(&bare_model());
        assert!(out.contains("▶ Coordinate Transform"));
        assert!(out.contains("  Transform: Not set"));
        assert!(!out.contains("Scale:"));
    }

    #[test]
    fn report_starts_with_a_section_and_ends_with_one_newline() {
        let out = render(&bare_model());
        assert!(out.starts_with("▶ File Details\n"));
        assert!(out.ends_with('\n'));
        assert!(!out.ends_with("\n\n"));
    }

    #[test]
    fn format_size_scales_by_unit() {
        assert_eq!(format_size(512), "512 bytes");
        assert_eq!(format_size(2048), "2.00 KB");
        assert_eq!(format_size(7_662_899), "7.31 MB");
        assert_eq!(format_size(3 * 1024 * 1024 * 1024), "3.00 GB");
    }
}
