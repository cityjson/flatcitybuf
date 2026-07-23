use cjseq::{CityJSON, CityJSONFeature, Transform as CjTransform};
use clap::{ArgAction, Parser, Subcommand};
use console::{style, Term};
use fcb_cli::CliError;
use fcb_core::error::Error;
use fcb_core::{
    attribute::{AttributeSchema, AttributeSchemaMethods},
    deserializer,
    header_writer::HeaderWriterOptions,
    FcbReader, FcbWriter,
};
use glob::glob;
use indicatif::{ProgressBar, ProgressStyle};
use std::{
    fs::File,
    io::{self, BufReader, BufWriter, Read, Write},
    path::PathBuf,
};
#[derive(Parser)]
#[command(
    name = "fcb",
    author,
    version,
    about = "CLI tool for CityJSON <-> FCB conversion"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Convert CityJSON to FCB
    Ser {
        /// Input files (glob patterns supported, e.g., "cities/*/*.jsonl")
        #[arg(short = 'i', long, required = true, num_args = 1..)]
        input: Vec<String>,

        /// Output file (use '-' for stdout)
        #[arg(short = 'o', long)]
        output: String,

        /// Comma-separated list of attributes to create index for
        #[arg(short = 'a', long)]
        attr_index: Option<String>,

        /// If index all attributes
        #[arg(short = 'A', long, action = ArgAction::SetTrue)]
        index_all_attributes: bool,

        /// Disable spatial index (spatial index is enabled by default)
        #[arg(short = 's', long, action = ArgAction::SetTrue)]
        no_spatial_index: bool,

        /// Branching factor for attribute index
        #[arg(long)]
        attr_branching_factor: Option<u16>,

        /// Node size of the spatial R-tree index (default 16)
        #[arg(long)]
        index_node_size: Option<u16>,

        /// Write a features_count of 0, which means "unknown" and forces
        /// readers to scan to EOF. Conformance fixtures only.
        #[arg(long, action = ArgAction::SetTrue)]
        no_feature_count: bool,

        /// Bounding box filter in format "minx,miny,maxx,maxy"
        #[arg(short = 'b', long)]
        bbox: Option<String>,

        /// Automatically calculate and set geospatial extent in header
        #[arg(short = 'g', long, action = ArgAction::SetTrue)]
        ge: bool,
    },

    /// Convert FCB to CityJSON
    Deser {
        /// Input file (use '-' for stdin)
        #[arg(short, long)]
        input: String,

        /// Output file (use '-' for stdout)
        #[arg(short, long)]
        output: String,
    },

    /// Convert CityJSON to CBOR
    Cbor {
        /// Input file (use '-' for stdin)
        #[arg(short, long)]
        input: String,
        /// Output file (use '-' for stdout)
        #[arg(short, long)]
        output: String,
    },

    /// Convert CityJSON to BSON
    Bson {
        /// Input file (use '-' for stdin)
        #[arg(short, long)]
        input: String,
        /// Output file (use '-' for stdout)
        #[arg(short, long)]
        output: String,
    },

    /// Show info about FCB file
    Info {
        /// Input FCB file
        #[arg(short, long)]
        input: PathBuf,
    },

    /// Interactively inspect an FCB file or URL in a terminal UI
    Inspect {
        /// Local path or HTTP(S) URL to an FCB file
        source: String,
    },
}

fn get_reader(input: &str) -> Result<Box<dyn Read>, Error> {
    match input {
        "-" => Ok(Box::new(io::stdin())),
        path => Ok(Box::new(File::open(path)?)),
    }
}

fn get_writer(output: &str) -> Result<Box<dyn Write>, Error> {
    match output {
        "-" => Ok(Box::new(io::stdout())),
        path => Ok(Box::new(File::create(path)?)),
    }
}

struct SerializeOptions {
    attr_index: Option<String>,
    index_all_attributes: bool,
    no_spatial_index: bool,
    attr_branching_factor: Option<u16>,
    index_node_size: Option<u16>,
    no_feature_count: bool,
    bbox: Option<String>,
    ge: bool,
}

fn serialize(inputs: &[String], output: &str, options: SerializeOptions) -> Result<(), CliError> {
    let term = Term::stderr();
    let is_stdout = output == "-";

    // Print header
    if !is_stdout {
        term.write_line(&format!(
            "\n{} {}",
            style("━━━").bold().cyan(),
            style("FlatCityBuf Serialization").bold().cyan()
        ))
        .ok();
        term.write_line(&format!(
            "{} {}",
            style("━━━").bold().cyan(),
            style("━━━━━━━━━━━━━━━━━━━━━━━━").bold().cyan()
        ))
        .ok();
    }

    // Expand glob patterns and collect all input files
    let mut input_paths: Vec<PathBuf> = Vec::new();
    for pattern in inputs {
        let paths: Vec<PathBuf> = glob(pattern)?.filter_map(|entry| entry.ok()).collect();
        if paths.is_empty() {
            // If no glob match, treat as literal path
            input_paths.push(PathBuf::from(pattern));
        } else {
            input_paths.extend(paths);
        }
    }

    if input_paths.is_empty() {
        return Err(CliError::NoInputFiles);
    }

    let writer = get_writer(output)?;
    let writer = BufWriter::new(writer);

    // Parse the bbox if provided
    let bbox_parsed = if let Some(bbox_str) = &options.bbox {
        Some(parse_bbox(bbox_str).map_err(|e| {
            CliError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("failed to parse bbox: {e}"),
            ))
        })?)
    } else {
        None
    };

    // Print configuration
    if !is_stdout {
        term.write_line("").ok();
        term.write_line(&format!("{} Configuration", style("▶").bold().green()))
            .ok();
        term.write_line(&format!(
            "  {} {} file(s)",
            style("Input:").dim(),
            style(input_paths.len()).yellow()
        ))
        .ok();
        for (i, path) in input_paths.iter().enumerate().take(5) {
            term.write_line(&format!(
                "    {}. {}",
                style(i + 1).dim(),
                style(path.display()).yellow()
            ))
            .ok();
        }
        if input_paths.len() > 5 {
            term.write_line(&format!(
                "    {} {} more files...",
                style("...").dim(),
                style(input_paths.len() - 5).dim()
            ))
            .ok();
        }
        term.write_line(&format!(
            "  {} {}",
            style("Output:").dim(),
            style(output).yellow()
        ))
        .ok();
        term.write_line(&format!(
            "  {} {}",
            style("Spatial Index:").dim(),
            if options.no_spatial_index {
                style("disabled").red()
            } else {
                style("enabled").green()
            }
        ))
        .ok();

        if let Some(bbox) = &bbox_parsed {
            term.write_line(&format!(
                "  {} [{:.2}, {:.2}, {:.2}, {:.2}]",
                style("Bounding Box:").dim(),
                bbox[0],
                bbox[1],
                bbox[2],
                bbox[3]
            ))
            .ok();
        }

        if options.index_all_attributes {
            term.write_line(&format!(
                "  {} {}",
                style("Attribute Index:").dim(),
                style("all attributes").green()
            ))
            .ok();
        } else if let Some(attrs) = &options.attr_index {
            term.write_line(&format!(
                "  {} {}",
                style("Attribute Index:").dim(),
                style(attrs).green()
            ))
            .ok();
        }

        if let Some(bf) = options.attr_branching_factor {
            term.write_line(&format!(
                "  {} {}",
                style("Branching Factor:").dim(),
                style(bf).yellow()
            ))
            .ok();
        }

        term.write_line(&format!(
            "  {} {}",
            style("Geospatial Extent:").dim(),
            if options.ge {
                style("auto-calculate").green()
            } else {
                style("not set").dim()
            }
        ))
        .ok();
        term.write_line("").ok();
    }

    // Read and merge input files
    if !is_stdout {
        term.write_line(&format!(
            "{} Reading CityJSON...",
            style("▶").bold().green()
        ))
        .ok();
    }

    let merge_result = fcb_cli::merger::merge_files(input_paths)?;
    let cj = merge_result.metadata;
    let features = merge_result.features;

    if !is_stdout {
        term.write_line(&format!(
            "  {} {} features",
            style("✓").bold().green(),
            style(features.len()).bold().yellow()
        ))
        .ok();
    }

    // Filter features by bbox if provided
    if !is_stdout && bbox_parsed.is_some() {
        term.write_line(&format!(
            "{} Filtering by bounding box...",
            style("▶").bold().green()
        ))
        .ok();
    }

    let filtered_features = if let Some(bbox) = &bbox_parsed {
        features
            .into_iter()
            .filter(|feature| feature_intersects_bbox(feature, bbox, &cj.transform))
            .collect()
    } else {
        features
    };

    if filtered_features.is_empty() {
        if !is_stdout {
            term.write_line(&format!(
                "  {} No features found within the specified bbox",
                style("⚠").bold().yellow()
            ))
            .ok();
        }
    } else if !is_stdout && bbox_parsed.is_some() {
        term.write_line(&format!(
            "  {} {} features after filtering",
            style("✓").bold().green(),
            style(filtered_features.len()).bold().yellow()
        ))
        .ok();
    }

    // Build attribute schema
    if !is_stdout {
        term.write_line(&format!(
            "{} Building attribute schema...",
            style("▶").bold().green()
        ))
        .ok();
    }

    let attr_schema = {
        let mut schema = AttributeSchema::new();
        // Limit to max 1000 features for schema building to have faster build time
        for feature in filtered_features.iter().take(1000) {
            // Sorted, because `add_attributes` assigns each new attribute the
            // next free column index -- so a `HashMap`'s random iteration order
            // would hand the same input different column numbers on every run.
            let mut ids: Vec<&String> = feature.city_objects.keys().collect();
            ids.sort_unstable();
            for co in ids
                .into_iter()
                .filter_map(|id| feature.city_objects.get(id))
            {
                if let Some(attributes) = &co.attributes {
                    schema.add_attributes(attributes);
                }
            }
        }
        if schema.is_empty() {
            None
        } else {
            Some(schema)
        }
    };

    if !is_stdout {
        if let Some(ref schema) = attr_schema {
            term.write_line(&format!(
                "  {} {} unique attributes found",
                style("✓").bold().green(),
                style(schema.len()).bold().yellow()
            ))
            .ok();
        } else {
            term.write_line(&format!(
                "  {} No attributes found",
                style("✓").bold().green()
            ))
            .ok();
        }
    }

    let semantic_attr_schema = {
        let mut schema = AttributeSchema::new();
        for feature in filtered_features.iter() {
            // Sorted for the same reason as the attribute schema above.
            let mut ids: Vec<&String> = feature.city_objects.keys().collect();
            ids.sort_unstable();
            for co in ids
                .into_iter()
                .filter_map(|id| feature.city_objects.get(id))
            {
                if let Some(geometry) = &co.geometry {
                    for geom in geometry.iter() {
                        if let Some(semantics) = geom.common().and_then(|c| c.semantics.as_ref()) {
                            for sem_obj in semantics.surfaces.iter() {
                                // A semantic surface's `other` holds the
                                // members the schema does not name; they
                                // become attribute columns.
                                if !sem_obj.other.is_empty() {
                                    let other = serde_json::Value::Object(
                                        sem_obj.other.clone().into_iter().collect(),
                                    );
                                    schema.add_attributes(&other);
                                }
                            }
                        }
                    }
                }
            }
        }
        if schema.is_empty() {
            None
        } else {
            Some(schema)
        }
    };

    let attr_index_vec: Option<Vec<(String, Option<u16>)>> =
        if options.index_all_attributes && attr_schema.is_some() {
            // create a vec with all attribute names and branching factor given
            Some(
                attr_schema
                    .clone()
                    .unwrap()
                    .iter()
                    .map(|attr| {
                        (
                            attr.0.to_string(),
                            Some(options.attr_branching_factor.unwrap_or(256)),
                        )
                    })
                    .collect::<Vec<(String, Option<u16>)>>(),
            )
        } else {
            options.attr_index.map(|s| {
                s.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .map(|s| (s, options.attr_branching_factor))
                    .collect::<Vec<(String, Option<u16>)>>()
            })
        };

    // Calculate geospatial extent if requested
    let geo_extent = if options.ge {
        if !is_stdout {
            term.write_line(&format!(
                "{} Calculating geospatial extent...",
                style("▶").bold().green()
            ))
            .ok();
        }
        let extent = calculate_geospatial_extent(&filtered_features, &cj.transform);
        if !is_stdout {
            term.write_line(&format!(
                "  {} Min: [{:.2}, {:.2}, {:.2}]",
                style("✓").bold().green(),
                extent[0],
                extent[1],
                extent[2]
            ))
            .ok();
            term.write_line(&format!(
                "    Max: [{:.2}, {:.2}, {:.2}]",
                extent[3], extent[4], extent[5]
            ))
            .ok();
        }
        Some(extent)
    } else {
        None
    };

    let header_options = HeaderWriterOptions {
        write_index: !options.no_spatial_index,
        feature_count: if options.no_feature_count {
            0
        } else {
            filtered_features.len() as u64
        },
        // The R-tree node size, NOT the attribute B+tree branching factor:
        // they are unrelated knobs and were previously driven by one flag.
        index_node_size: options.index_node_size.unwrap_or(16),
        attribute_indices: attr_index_vec.clone(),
        geographical_extent: geo_extent,
    };

    // Show index information
    if !is_stdout {
        term.write_line(&format!(
            "{} Building indices...",
            style("▶").bold().green()
        ))
        .ok();

        if !options.no_spatial_index {
            term.write_line(&format!(
                "  {} Spatial R-tree index (node size: {})",
                style("✓").bold().green(),
                style(header_options.index_node_size).yellow()
            ))
            .ok();
        }

        if let Some(ref indices) = attr_index_vec {
            term.write_line(&format!(
                "  {} Attribute B+Tree indices for {} attributes:",
                style("✓").bold().green(),
                style(indices.len()).yellow()
            ))
            .ok();
            for (attr_name, bf) in indices.iter().take(5) {
                term.write_line(&format!(
                    "    • {} (branching factor: {})",
                    style(attr_name).cyan(),
                    style(bf.unwrap_or(16)).dim()
                ))
                .ok();
            }
            if indices.len() > 5 {
                term.write_line(&format!(
                    "    {} {} more attributes...",
                    style("...").dim(),
                    style(indices.len() - 5).dim()
                ))
                .ok();
            }
        }
        term.write_line("").ok();
    }

    // Write features
    if !is_stdout {
        term.write_line(&format!(
            "{} Writing FCB file...",
            style("▶").bold().green()
        ))
        .ok();
    }

    let mut fcb = FcbWriter::new(cj, Some(header_options), attr_schema, semantic_attr_schema)?;

    let pb = if !is_stdout {
        let pb = ProgressBar::new(filtered_features.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("  {bar:40.cyan/blue} {pos}/{len} features ({percent}%)")
                .unwrap()
                .progress_chars("━━╾─"),
        );
        Some(pb)
    } else {
        None
    };

    for feature in filtered_features.iter() {
        fcb.add_feature(feature)?;
        if let Some(ref pb) = pb {
            pb.inc(1);
        }
    }

    if let Some(ref pb) = pb {
        pb.finish_and_clear();
    }

    fcb.write(writer)?;

    if !is_stdout {
        term.write_line(&format!(
            "  {} File written successfully",
            style("✓").bold().green()
        ))
        .ok();
        term.write_line("").ok();
        term.write_line(&format!(
            "{} {}",
            style("━━━").bold().cyan(),
            style("Serialization Complete").bold().cyan()
        ))
        .ok();
        term.write_line(&format!(
            "{} {}",
            style("━━━").bold().cyan(),
            style("━━━━━━━━━━━━━━━━━━━━━━").bold().cyan()
        ))
        .ok();
        term.write_line("").ok();
    }

    Ok(())
}

/// Parse a bounding box string in format "minx,miny,maxx,maxy"
fn parse_bbox(bbox_str: &str) -> Result<[f64; 4], String> {
    let parts: Vec<&str> = bbox_str.split(',').collect();
    if parts.len() != 4 {
        return Err(format!(
            "Invalid bounding box format. Expected 'minx,miny,maxx,maxy', got '{bbox_str}'"
        ));
    }

    let mut bbox = [0.0; 4];
    for (i, part) in parts.iter().enumerate() {
        bbox[i] = part
            .trim()
            .parse::<f64>()
            .map_err(|e| format!("Failed to parse bbox component: {e}"))?;
    }

    // Validate that min <= max
    if bbox[0] > bbox[2] || bbox[1] > bbox[3] {
        return Err(
            "Invalid bounding box: min values must be less than or equal to max values".to_string(),
        );
    }

    Ok(bbox)
}

/// Get all vertices from a feature
fn get_vertices_from_feature(feature: &CityJSONFeature, transform: &CjTransform) -> Vec<[f64; 3]> {
    let mut result = Vec::new();

    for vertex in &feature.vertices {
        if vertex.len() >= 3 {
            // Convert from i64 to f64 and apply transform
            let x = (vertex[0] as f64 * transform.scale[0]) + transform.translate[0];
            let y = (vertex[1] as f64 * transform.scale[1]) + transform.translate[1];
            let z = (vertex[2] as f64 * transform.scale[2]) + transform.translate[2];

            result.push([x, y, z]);
        }
    }

    result
}

/// Check if a CityJSONFeature intersects with a bounding box
fn feature_intersects_bbox(
    feature: &CityJSONFeature,
    bbox: &[f64; 4],
    transform: &CjTransform,
) -> bool {
    // Get transformed vertices from the feature
    let vertices = get_vertices_from_feature(feature, transform);
    if city_object_intersects_bbox(bbox, &vertices) {
        return true;
    }

    false
}

/// Check if a CityObject intersects with a bounding box
fn city_object_intersects_bbox(bbox: &[f64; 4], feature_vertices: &[[f64; 3]]) -> bool {
    // Check if any of the vertices are within the bbox
    for vertex in feature_vertices {
        if point_in_bbox_2d(vertex, bbox) {
            return true;
        }
    }

    false
}

/// Check if a point is inside a 2D bounding box
fn point_in_bbox_2d(point: &[f64; 3], bbox: &[f64; 4]) -> bool {
    point[0] >= bbox[0] && point[0] <= bbox[2] && point[1] >= bbox[1] && point[1] <= bbox[3]
}

/// Calculate the geospatial extent from a list of features
fn calculate_geospatial_extent(features: &[CityJSONFeature], transform: &CjTransform) -> [f64; 6] {
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut min_z = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    let mut max_z = f64::MIN;

    for feature in features {
        let vertices = get_vertices_from_feature(feature, transform);

        for [x, y, z] in vertices {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            min_z = min_z.min(z);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
            max_z = max_z.max(z);
        }
    }

    // If no vertices were found, return a default extent
    if min_x == f64::MAX {
        return [0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    }

    [min_x, min_y, min_z, max_x, max_y, max_z]
}

fn deserialize(input: &str, output: &str) -> Result<(), Error> {
    let reader = BufReader::new(get_reader(input)?);
    let mut writer = BufWriter::new(get_writer(output)?);
    let mut fcb_reader = FcbReader::open(reader)?.select_all_seq()?;

    let header = fcb_reader.header();
    let cj = deserializer::to_cj_metadata(&header)?;

    // Write header
    writeln!(writer, "{}", serde_json::to_string(&cj)?)?;

    // Write features. The iterator stops at the declared feature count, or at
    // EOF when the header declares 0, which means "unknown". Breaking here on
    // `features_count` instead truncated such a file to a single feature.
    // `?` on the iterator, not `while let Ok(..)`: swallowing the error made a
    // mid-file decode failure indistinguishable from a clean end of stream, so
    // a count-0 file could be truncated to a short, wrong output and still exit 0.
    while let Some(feat_buf) = fcb_reader.next()? {
        let feature = feat_buf.cur_cj_feature()?;
        writeln!(writer, "{}", serde_json::to_string(&feature)?)?;
    }

    if output != "-" {
        eprintln!("Successfully decoded to CityJSON");
    }
    Ok(())
}

fn encode_cbor(input: &str, output: &str) -> Result<(), Error> {
    let reader = BufReader::new(get_reader(input)?);
    let writer = BufWriter::new(get_writer(output)?);

    let value: serde_json::Value = serde_json::from_reader(reader)?;
    serde_cbor::to_writer(writer, &value).map_err(|e| {
        Error::IoError(std::io::Error::other(format!(
            "failed to encode to cbor: {e}"
        )))
    })?;

    if output != "-" {
        eprintln!("successfully encoded to cbor");
    }
    Ok(())
}

fn encode_bson(input: &str, output: &str) -> Result<(), Error> {
    let mut reader = BufReader::new(get_reader(input)?);
    let json_str = {
        let mut s = String::new();
        reader.read_to_string(&mut s)?;
        s
    };

    let cityjson: CityJSON = serde_json::from_str(&json_str)?;
    let bson = bson::to_bson(&cityjson).map_err(|e| {
        Error::IoError(std::io::Error::other(format!(
            "failed to encode to bson: {e}"
        )))
    })?;
    let doc = bson.as_document().unwrap();

    let mut writer = get_writer(output)?;
    doc.to_writer(&mut writer).map_err(|e| {
        Error::IoError(std::io::Error::other(format!(
            "failed to encode to bson: {e}"
        )))
    })?;

    if output != "-" {
        eprintln!("successfully encoded to bson");
    }
    Ok(())
}

fn show_info(input: PathBuf) -> Result<(), Error> {
    let term = Term::stdout();

    // Print header
    term.write_line(&format!(
        "\n{} {}",
        style("━━━").bold().cyan(),
        style("FlatCityBuf File Information").bold().cyan()
    ))
    .ok();
    term.write_line(&format!(
        "{} {}",
        style("━━━").bold().cyan(),
        style("━━━━━━━━━━━━━━━━━━━━━━━━━━━").bold().cyan()
    ))
    .ok();
    term.write_line("").ok();

    let reader = BufReader::new(File::open(&input)?);
    let file_size = reader.get_ref().metadata()?.len();
    let fcb_reader = FcbReader::open(reader)?.select_all()?;
    let header = fcb_reader.header();

    // File information
    term.write_line(&format!("{} File Details", style("▶").bold().green()))
        .ok();
    term.write_line(&format!(
        "  {} {}",
        style("Path:").dim(),
        style(input.display()).yellow()
    ))
    .ok();

    // Format file size nicely
    let size_str = if file_size >= 1024 * 1024 * 1024 {
        format!("{:.2} GB", file_size as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if file_size >= 1024 * 1024 {
        format!("{:.2} MB", file_size as f64 / (1024.0 * 1024.0))
    } else if file_size >= 1024 {
        format!("{:.2} KB", file_size as f64 / 1024.0)
    } else {
        format!("{} bytes", file_size)
    };

    term.write_line(&format!(
        "  {} {}",
        style("Size:").dim(),
        style(size_str).yellow()
    ))
    .ok();
    term.write_line(&format!(
        "  {} {}",
        style("Version:").dim(),
        style(header.version()).yellow()
    ))
    .ok();

    if let Some(title) = header.title() {
        term.write_line(&format!(
            "  {} {}",
            style("Title:").dim(),
            style(title).yellow()
        ))
        .ok();
    }

    term.write_line("").ok();

    // Dataset information
    term.write_line(&format!("{} Dataset", style("▶").bold().green()))
        .ok();
    term.write_line(&format!(
        "  {} {}",
        style("Features:").dim(),
        style(header.features_count()).bold().yellow()
    ))
    .ok();

    if let Some(extent) = header.geographical_extent() {
        term.write_line(&format!("  {} Yes", style("Geospatial Extent:").dim()))
            .ok();
        term.write_line(&format!(
            "    {} [{:.2}, {:.2}, {:.2}]",
            style("Min:").dim(),
            extent.min().x(),
            extent.min().y(),
            extent.min().z()
        ))
        .ok();
        term.write_line(&format!(
            "    {} [{:.2}, {:.2}, {:.2}]",
            style("Max:").dim(),
            extent.max().x(),
            extent.max().y(),
            extent.max().z()
        ))
        .ok();

        // Calculate dimensions
        let width = extent.max().x() - extent.min().x();
        let height = extent.max().y() - extent.min().y();
        let depth = extent.max().z() - extent.min().z();
        term.write_line(&format!(
            "    {} {:.2} × {:.2} × {:.2}",
            style("Dimensions:").dim(),
            width,
            height,
            depth
        ))
        .ok();
    } else {
        term.write_line(&format!(
            "  {} {}",
            style("Geospatial Extent:").dim(),
            style("Not set").dim()
        ))
        .ok();
    }

    term.write_line("").ok();

    // Index information
    term.write_line(&format!("{} Indices", style("▶").bold().green()))
        .ok();

    let has_spatial_index = header.index_node_size() > 0;
    term.write_line(&format!(
        "  {} {}",
        style("Spatial R-tree:").dim(),
        if has_spatial_index {
            style("Yes").green()
        } else {
            style("No").red()
        }
    ))
    .ok();

    let raw_attr_index = header.attribute_index();
    if let Some(ai_vec) = raw_attr_index {
        let attr_names: Vec<String> = ai_vec
            .iter()
            .filter_map(|ai| {
                header
                    .columns()
                    .iter()
                    .flat_map(|c| c.iter())
                    .find(|ci| ci.index() == ai.index())
                    .map(|ci| ci.name().to_string())
            })
            .collect();

        term.write_line(&format!(
            "  {} {} (B+Tree)",
            style("Attribute Indices:").dim(),
            style(attr_names.len()).yellow()
        ))
        .ok();

        if !attr_names.is_empty() {
            for (i, name) in attr_names.iter().enumerate().take(10) {
                term.write_line(&format!(
                    "    {}. {}",
                    style(i + 1).dim(),
                    style(name).cyan()
                ))
                .ok();
            }
            if attr_names.len() > 10 {
                term.write_line(&format!(
                    "    {} {} more attributes...",
                    style("...").dim(),
                    style(attr_names.len() - 10).dim()
                ))
                .ok();
            }
        }
    } else {
        term.write_line(&format!(
            "  {} {}",
            style("Attribute Indices:").dim(),
            style("None").dim()
        ))
        .ok();
    }

    term.write_line("").ok();

    // Transform information
    if let Some(transform) = header.transform() {
        term.write_line(&format!(
            "{} Coordinate Transform",
            style("▶").bold().green()
        ))
        .ok();
        term.write_line(&format!(
            "  {} [{:.6}, {:.6}, {:.6}]",
            style("Scale:").dim(),
            transform.scale().x(),
            transform.scale().y(),
            transform.scale().z()
        ))
        .ok();
        term.write_line(&format!(
            "  {} [{:.6}, {:.6}, {:.6}]",
            style("Translate:").dim(),
            transform.translate().x(),
            transform.translate().y(),
            transform.translate().z()
        ))
        .ok();
        term.write_line("").ok();
    }

    // Footer
    term.write_line(&format!(
        "{} {}",
        style("━━━").bold().cyan(),
        style("━━━━━━━━━━━━━━━━━━━━━━━━━━━").bold().cyan()
    ))
    .ok();
    term.write_line("").ok();

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Ser {
            input,
            output,
            attr_index,
            index_all_attributes,
            no_spatial_index,
            attr_branching_factor,
            index_node_size,
            no_feature_count,
            bbox,
            ge,
        } => serialize(
            &input,
            &output,
            SerializeOptions {
                attr_index,
                index_all_attributes,
                no_spatial_index,
                attr_branching_factor,
                index_node_size,
                no_feature_count,
                bbox,
                ge,
            },
        )?,
        Commands::Deser { input, output } => deserialize(&input, &output)?,
        Commands::Cbor { input, output } => encode_cbor(&input, &output)?,
        Commands::Bson { input, output } => encode_bson(&input, &output)?,
        Commands::Info { input } => show_info(input)?,
        Commands::Inspect { source } => {
            if let Err(err) = fcb_cli::inspect::run_inspect(&source) {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_cli() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
