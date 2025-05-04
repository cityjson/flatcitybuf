use cjseq::{CityJSON, CityJSONFeature, Transform as CjTransform};
use clap::{Parser, Subcommand};
use fcb_core::error::Error;
use fcb_core::{
    attribute::{AttributeSchema, AttributeSchemaMethods},
    deserializer,
    header_writer::HeaderWriterOptions,
    read_cityjson_from_reader, CJType, CJTypeKind, CityJSONSeq, FcbReader, FcbWriter,
};
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
        /// Input file (use '-' for stdin)
        #[arg(short = 'i', long)]
        input: String,

        /// Output file (use '-' for stdout)
        #[arg(short = 'o', long)]
        output: String,

        /// Comma-separated list of attributes to create index for
        #[arg(short = 'a', long)]
        attr_index: Option<String>,

        /// If index all attributes
        #[arg(short = 'A', long)]
        index_all_attributes: bool,

        /// Branching factor for attribute index
        #[arg(long)]
        attr_branching_factor: Option<u16>,

        /// Bounding box filter in format "minx,miny,maxx,maxy"
        #[arg(short = 'b', long)]
        bbox: Option<String>,

        /// Automatically calculate and set geospatial extent in header
        #[arg(short = 'g', long)]
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

fn serialize(
    input: &str,
    output: &str,
    attr_index: Option<String>,
    index_all_attributes: bool,
    attr_branching_factor: Option<u16>,
    bbox: Option<String>,
    ge: bool,
) -> Result<(), Error> {
    let reader = get_reader(input)?;
    let writer = get_writer(output)?;

    let reader = BufReader::new(reader);
    let writer = BufWriter::new(writer);

    // Parse the bbox if provided
    let bbox_parsed = if let Some(bbox_str) = bbox {
        Some(parse_bbox(&bbox_str).map_err(|e| {
            Error::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("failed to parse bbox: {}", e),
            ))
        })?)
    } else {
        None
    };

    // Create a CityJSONSeq reader
    let cj_seq = match read_cityjson_from_reader(reader, CJTypeKind::Seq) {
        Ok(CJType::Seq(seq)) => seq,
        _ => {
            return Err(Error::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                "failed to read CityJSON Feature",
            )))
        }
    };

    let CityJSONSeq { cj, features } = cj_seq;

    // Filter features by bbox if provided
    let filtered_features = if let Some(bbox) = &bbox_parsed {
        features
            .into_iter()
            .filter(|feature| feature_intersects_bbox(feature, bbox, &cj.transform))
            .collect()
    } else {
        features
    };

    if filtered_features.is_empty() {
        eprintln!("warning: no features found within the specified bbox");
    }

    let attr_schema = {
        let mut schema = AttributeSchema::new();
        // Limit to max 1000 features for schema building to have faster build time
        for feature in filtered_features.iter().take(1000) {
            for (_, co) in feature.city_objects.iter() {
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

    let attr_index_vec: Option<Vec<(String, Option<u16>)>> =
        if index_all_attributes && attr_schema.is_some() {
            // create a vec with all attribute names and branching factor given
            Some(
                attr_schema
                    .clone()
                    .unwrap()
                    .iter()
                    .map(|attr| {
                        (
                            attr.0.to_string(),
                            Some(attr_branching_factor.unwrap_or(256)),
                        )
                    })
                    .collect::<Vec<(String, Option<u16>)>>(),
            )
        } else {
            attr_index.map(|s| {
                s.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .map(|s| (s, attr_branching_factor))
                    .collect::<Vec<(String, Option<u16>)>>()
            })
        };

    // Calculate geospatial extent if requested
    let geo_extent = if ge {
        Some(calculate_geospatial_extent(
            &filtered_features,
            &cj.transform,
        ))
    } else {
        None
    };

    let header_options = HeaderWriterOptions {
        write_index: true,
        feature_count: filtered_features.len() as u64,
        index_node_size: 16,
        attribute_indices: attr_index_vec,
        geographical_extent: geo_extent,
    };

    println!("header_options in cli: {:?}", header_options);

    let mut fcb = FcbWriter::new(cj, Some(header_options), attr_schema)?;

    for feature in filtered_features.iter() {
        fcb.add_feature(feature)?;
    }
    fcb.write(writer)?;

    if output != "-" {
        eprintln!("Successfully encoded to FCB");
    }

    Ok(())
}

/// Parse a bounding box string in format "minx,miny,maxx,maxy"
fn parse_bbox(bbox_str: &str) -> Result<[f64; 4], String> {
    let parts: Vec<&str> = bbox_str.split(',').collect();
    if parts.len() != 4 {
        return Err(format!(
            "Invalid bounding box format. Expected 'minx,miny,maxx,maxy', got '{}'",
            bbox_str
        ));
    }

    let mut bbox = [0.0; 4];
    for (i, part) in parts.iter().enumerate() {
        bbox[i] = part
            .trim()
            .parse::<f64>()
            .map_err(|e| format!("Failed to parse bbox component: {}", e))?;
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

    // Write features
    let feat_count = header.features_count();
    let mut feat_num = 0;
    while let Ok(Some(feat_buf)) = fcb_reader.next() {
        let feature = feat_buf.cur_cj_feature()?;
        writeln!(writer, "{}", serde_json::to_string(&feature)?)?;

        feat_num += 1;
        if feat_num >= feat_count {
            break;
        }
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
        Error::IoError(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("failed to encode to cbor: {}", e),
        ))
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
        Error::IoError(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("failed to encode to bson: {}", e),
        ))
    })?;
    let doc = bson.as_document().unwrap();

    let mut writer = get_writer(output)?;
    doc.to_writer(&mut writer).map_err(|e| {
        Error::IoError(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("failed to encode to bson: {}", e),
        ))
    })?;

    if output != "-" {
        eprintln!("successfully encoded to bson");
    }
    Ok(())
}

fn show_info(input: PathBuf) -> Result<(), Error> {
    let reader = BufReader::new(File::open(input)?);
    let metadata = reader.get_ref().metadata()?.len() / 1024 / 1024; // show in megabytes
    let fcb_reader = FcbReader::open(reader)?.select_all()?;
    let raw_attr_index = fcb_reader.header().attribute_index();
    let attr_index = raw_attr_index.map(|ai_vec| {
        ai_vec
            .iter()
            .map(|ai| {
                fcb_reader
                    .header()
                    .columns()
                    .iter()
                    .flat_map(|c| c.iter())
                    .find(|ci| ci.index() == ai.index())
                    .map(|ci| ci.name())
                    .unwrap()
            })
            .collect::<Vec<_>>()
    });
    let header = fcb_reader.header();
    println!("FCB File Info:");
    println!("    File size: {} MB", metadata);
    println!("  Version: {}", header.version());
    println!("  Features count: {}", header.features_count());
    println!("  bbox: {:?}", header.geographical_extent());
    println!("  attr_index: {:?}", attr_index.unwrap_or_default());

    if let Some(title) = header.title() {
        println!("  Title: {}", title);
    }

    if let Some(extent) = header.geographical_extent() {
        println!("  Geographical extent:");
        println!(
            "    Min: [{}, {}, {}]",
            extent.min().x(),
            extent.min().y(),
            extent.min().z()
        );
        println!(
            "    Max: [{}, {}, {}]",
            extent.max().x(),
            extent.max().y(),
            extent.max().z()
        );
    }

    Ok(())
}

fn main() -> Result<(), Error> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Ser {
            input,
            output,
            attr_index,
            index_all_attributes,
            attr_branching_factor,
            bbox,
            ge,
        } => serialize(
            &input,
            &output,
            attr_index,
            index_all_attributes,
            attr_branching_factor,
            bbox,
            ge,
        ),
        Commands::Deser { input, output } => deserialize(&input, &output),
        Commands::Cbor { input, output } => encode_cbor(&input, &output),
        Commands::Bson { input, output } => encode_bson(&input, &output),
        Commands::Info { input } => show_info(input),
    }
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
