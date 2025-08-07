use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use fcb_core::FcbReader;
use prettytable::{Cell, Row, Table};
use std::{
    collections::HashSet,
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

#[derive(Debug, Parser)]
#[command(
    name = "fcb_stats",
    about = "Calculate statistics for FlatCityBuf files compared to CityJSONSeq"
)]
struct Args {
    /// Directory containing pairs of .fcb and .city.jsonl files (defaults to current directory)
    #[arg(short, long, default_value = ".")]
    dir: PathBuf,

    /// Recursively search subdirectories for files
    #[arg(short, long)]
    recursive: bool,

    /// Output format (table by default)
    #[arg(short, long, value_enum, default_value_t = OutputFormat::Table)]
    format: OutputFormat,

    /// List of specific city prefixes to process (e.g., "NYC,Zurich")
    #[arg(short, long)]
    cities: Option<String>,
}

#[derive(Debug, Clone, ValueEnum)]
enum OutputFormat {
    /// Human-readable table
    Table,
    /// CSV format
    Csv,
    /// JSON format
    Json,
}

#[derive(Debug)]
struct FileStats {
    city_name: String,
    fcb_file_size: u64,
    jsonl_file_size: u64,
    compression_factor: f64,
    feature_count: u64,
    city_object_count: usize,
    vertex_count: usize,
    attribute_count: usize,
    semantic_attribute_count: usize,
    avg_vertices_per_object: f64,
    avg_vertices_per_feature: f64,
    avg_feature_size: f64,
    avg_cjseq_feature_size: f64,
    feature_compression_factor: f64,
}

fn format_size(size_bytes: u64) -> String {
    if size_bytes < 1024 {
        format!("{size_bytes} B")
    } else if size_bytes < 1024 * 1024 {
        format!("{:.2} KB", size_bytes as f64 / 1024.0)
    } else if size_bytes < 1024 * 1024 * 1024 {
        format!("{:.2} MB", size_bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", size_bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Calculate statistics for an FCB file
fn analyze_fcb(path: &Path) -> Result<(u64, usize, usize, usize, usize, f64)> {
    println!("analyzing fcb file: {}", path.display());

    let file = File::open(path).context("failed to open fcb file")?;
    let file_size = file.metadata().context("failed to get fcb metadata")?.len();
    let reader = BufReader::new(file);

    let mut fcb_reader = FcbReader::open(reader)
        .context("failed to open fcb reader")?
        .select_all()
        .context("failed to select all from fcb reader")?;

    let header = fcb_reader.header();
    let feature_count = header.features_count();

    // Get attribute count from the header's columns
    let attribute_count = header.columns().map(|col| col.len()).unwrap_or(0);
    let semantic_attribute_count = header.semantic_columns().map(|col| col.len()).unwrap_or(0);

    // Count city objects and vertices
    let mut city_object_count = 0;
    let mut vertex_count = 0;
    let mut feat_num = 0;
    let mut total_feature_size = 0;

    while let Some(feat_buf) = fcb_reader.next().context("failed to read next feature")? {
        let feature = feat_buf.cur_feature();
        // let cj_feature = feat_buf.cur_cj_feature()?;
        // for (co_id, co) in cj_feature.city_objects {
        //     for geom in co.geometry.unwrap_or_default().iter() {
        //         if let Some(sem) = geom.semantics.as_ref() {
        //             println!("sem: {:?}", sem);
        //         }
        //     }
        // }
        total_feature_size += feat_buf.cur_feature_len();

        // Count vertices in this feature
        if let Some(vertices) = feature.vertices() {
            vertex_count += vertices.len();
        }

        // Count city objects
        if let Some(objects) = feature.objects() {
            for co in objects.iter() {
                for geom in co.geometry().unwrap_or_default().iter() {
                    if let Some(sem) = geom.semantics_objects() {
                        for sem_obj in sem.iter() {
                            // println!("sem_obj: {:?}", sem_obj);
                        }
                    }
                }
            }
            city_object_count += objects.len();
        }

        feat_num += 1;
        if feat_num == feature_count {
            break;
        }
    }
    let avg_feature_size = total_feature_size as f64 / feature_count as f64;

    Ok((
        feature_count,
        city_object_count,
        vertex_count,
        attribute_count,
        semantic_attribute_count,
        avg_feature_size,
    ))
}

/// Analyze CityJSONSeq file to determine file size and average feature size
fn analyze_jsonl(path: &Path) -> Result<(u64, f64)> {
    println!("analyzing jsonl file: {}", path.display());

    let file = File::open(path).context("failed to open jsonl file")?;
    let file_size = file
        .metadata()
        .context("failed to get jsonl metadata")?
        .len();

    // Calculate average feature size by reading each line
    let reader =
        BufReader::new(File::open(path).context("failed to open jsonl file for feature analysis")?);
    let mut lines = reader.lines();

    // Skip header line (CityJSON metadata)
    if let Some(Ok(_)) = lines.next() {
        // Now count each feature and its size
        let mut feature_count = 0;
        let mut total_feature_size = 0;

        for line in lines {
            if let Ok(line_content) = line {
                total_feature_size += line_content.len();
                feature_count += 1;
            }
        }

        let avg_feature_size = if feature_count > 0 {
            total_feature_size as f64 / feature_count as f64
        } else {
            0.0
        };

        Ok((file_size, avg_feature_size))
    } else {
        // If we can't read the header, just return the file size and 0 avg
        Ok((file_size, 0.0))
    }
}

/// Recursively find all FCB files in directory and subdirectories
fn find_fcb_files(
    dir: &Path,
    recursive: bool,
    filter_cities: Option<&HashSet<String>>,
    fcb_files: &mut Vec<(PathBuf, String)>,
) -> Result<()> {
    for entry in
        std::fs::read_dir(dir).context(format!("failed to read directory: {}", dir.display()))?
    {
        let entry = entry.context("failed to read directory entry")?;
        let path = entry.path();

        if path.is_dir() && recursive {
            // Recursively search subdirectories if requested
            find_fcb_files(&path, recursive, filter_cities, fcb_files)?;
        } else if let Some(ext) = path.extension() {
            if ext == "fcb" {
                if let Some(file_stem) = path.file_stem() {
                    let city_name = file_stem.to_string_lossy().to_string();

                    // If we have a filter and this city isn't in it, skip
                    if let Some(filter) = filter_cities {
                        if !filter.iter().any(|prefix| city_name.starts_with(prefix)) {
                            continue;
                        }
                    }

                    fcb_files.push((path, city_name));
                }
            }
        }
    }

    Ok(())
}

/// Find all dataset pairs in a directory
fn find_dataset_pairs(
    dir: &Path,
    recursive: bool,
    filter_cities: Option<&HashSet<String>>,
) -> Result<Vec<(PathBuf, PathBuf, String)>> {
    let mut pairs = Vec::new();
    let mut fcb_files = Vec::new();

    // Find all FCB files
    find_fcb_files(dir, recursive, filter_cities, &mut fcb_files)?;

    // Now find matching JSONL files for each FCB file
    for (fcb_path, city_name) in fcb_files {
        // Get the directory containing the FCB file
        let parent_dir = fcb_path.parent().unwrap_or(Path::new(""));

        // Look for a .city.jsonl file with the same base name
        let jsonl_name = format!("{city_name}.city.jsonl");
        let jsonl_path = parent_dir.join(&jsonl_name);

        if jsonl_path.exists() {
            pairs.push((fcb_path, jsonl_path, city_name));
        } else {
            // Try alternative naming pattern (.jsonl without .city)
            let alt_jsonl_name = format!("{city_name}.jsonl");
            let alt_jsonl_path = parent_dir.join(&alt_jsonl_name);

            if alt_jsonl_path.exists() {
                pairs.push((fcb_path, alt_jsonl_path, city_name));
            } else {
                println!("warning: no matching jsonl file found for {city_name}");
            }
        }
    }

    Ok(pairs)
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Parse city filter if provided
    let filter_cities = args.cities.map(|cities_str| {
        cities_str
            .split(',')
            .map(|s| s.trim().to_string())
            .collect::<HashSet<String>>()
    });

    // Find all pairs of FCB and JSONL files
    let pairs = find_dataset_pairs(&args.dir, args.recursive, filter_cities.as_ref())?;
    if pairs.is_empty() {
        println!("no matching dataset pairs found in {}", args.dir.display());
        return Ok(());
    }

    println!("found {} dataset pairs", pairs.len());

    // Analyze each pair
    let mut stats = Vec::new();
    for (fcb_path, jsonl_path, city_name) in pairs {
        println!("processing city: {city_name}");

        // Analyze FCB file
        let (
            feature_count,
            city_object_count,
            vertex_count,
            attribute_count,
            semantic_attribute_count,
            avg_feature_size,
        ) = match analyze_fcb(&fcb_path) {
            Ok(stats) => stats,
            Err(e) => {
                println!("error analyzing {}: {}", fcb_path.display(), e);
                continue;
            }
        };

        // Get file sizes
        let fcb_file_size = match File::open(&fcb_path) {
            Ok(file) => match file.metadata() {
                Ok(metadata) => metadata.len(),
                Err(e) => {
                    println!("error getting metadata for {}: {}", fcb_path.display(), e);
                    continue;
                }
            },
            Err(e) => {
                println!("error opening {}: {}", fcb_path.display(), e);
                continue;
            }
        };

        let (jsonl_file_size, avg_cjseq_feature_size) = match analyze_jsonl(&jsonl_path) {
            Ok((size, avg)) => (size, avg),
            Err(e) => {
                println!("error analyzing {}: {}", jsonl_path.display(), e);
                continue;
            }
        };

        // Calculate compression factor
        let compression_factor = if jsonl_file_size > 0 {
            (jsonl_file_size as f64 - fcb_file_size as f64) / jsonl_file_size as f64
        } else {
            0.0
        };

        // Calculate average vertices per city object
        let avg_vertices_per_object = if city_object_count > 0 {
            vertex_count as f64 / city_object_count as f64
        } else {
            0.0
        };

        let avg_vertices_per_feature = if feature_count > 0 {
            vertex_count as f64 / feature_count as f64
        } else {
            0.0
        };

        // Calculate feature-level compression factor
        let feature_compression_factor = if avg_cjseq_feature_size > 0.0 {
            (avg_cjseq_feature_size - avg_feature_size) / avg_cjseq_feature_size
        } else {
            0.0
        };

        stats.push(FileStats {
            city_name,
            fcb_file_size,
            jsonl_file_size,
            compression_factor,
            feature_count,
            city_object_count,
            vertex_count,
            attribute_count,
            semantic_attribute_count,
            avg_vertices_per_object,
            avg_vertices_per_feature,
            avg_feature_size,
            avg_cjseq_feature_size,
            feature_compression_factor,
        });
    }

    // Sort stats by city name for consistent output
    stats.sort_by(|a, b| a.city_name.cmp(&b.city_name));

    // Output results based on selected format
    match args.format {
        OutputFormat::Table => output_table(&stats),
        OutputFormat::Csv => output_csv(&stats)?,
        OutputFormat::Json => output_json(&stats)?,
    }

    Ok(())
}

fn output_table(stats: &[FileStats]) {
    let mut table = Table::new();

    // Add header row
    table.add_row(Row::new(vec![
        Cell::new("City"),
        Cell::new("FCB Size"),
        Cell::new("JSONL Size"),
        Cell::new("Compression"),
        Cell::new("Features"),
        Cell::new("City Objects"),
        Cell::new("Vertices"),
        Cell::new("Vertices/Object"),
        Cell::new("Vertices/Feature"),
        Cell::new("Attributes"),
        Cell::new("Semantic Attributes"),
        Cell::new("Avg FCB Feat Size"),
        Cell::new("Avg JSONL Feat Size"),
        Cell::new("Feature Compression"),
    ]));

    // Add data rows
    for stat in stats {
        table.add_row(Row::new(vec![
            Cell::new(&stat.city_name),
            Cell::new(&format_size(stat.fcb_file_size)),
            Cell::new(&format_size(stat.jsonl_file_size)),
            Cell::new(&format!("{:.2}%", stat.compression_factor * 100.0)),
            Cell::new(&stat.feature_count.to_string()),
            Cell::new(&stat.city_object_count.to_string()),
            Cell::new(&stat.vertex_count.to_string()),
            Cell::new(&format!("{:.2}", stat.avg_vertices_per_object)),
            Cell::new(&format!("{:.2}", stat.avg_vertices_per_feature)),
            Cell::new(&stat.attribute_count.to_string()),
            Cell::new(&stat.semantic_attribute_count.to_string()),
            Cell::new(&format_size(stat.avg_feature_size as u64)),
            Cell::new(&format_size(stat.avg_cjseq_feature_size as u64)),
            Cell::new(&format!("{:.2}%", stat.feature_compression_factor * 100.0)),
        ]));
    }

    // Print the table
    table.printstd();
}

fn output_csv(stats: &[FileStats]) -> Result<()> {
    println!("city,fcb_size,jsonl_size,compression,features,city_objects,vertices,vertices_per_object,attributes,avg_fcb_feature_size,avg_jsonl_feature_size,feature_compression");

    for stat in stats {
        println!(
            "{},{},{},{:.4},{},{},{},{:.2},{:.2},{:.2},{:.2},{:.2},{:.4},{:.2}",
            stat.city_name,
            stat.fcb_file_size,
            stat.jsonl_file_size,
            stat.compression_factor,
            stat.feature_count,
            stat.city_object_count,
            stat.vertex_count,
            stat.avg_vertices_per_object,
            stat.avg_vertices_per_feature,
            stat.attribute_count,
            stat.semantic_attribute_count,
            stat.avg_feature_size,
            stat.avg_cjseq_feature_size,
            stat.feature_compression_factor
        );
    }

    Ok(())
}

fn output_json(stats: &[FileStats]) -> Result<()> {
    // Convert stats to JSON
    let json = serde_json::to_string_pretty(
        &stats
            .iter()
            .map(|s| {
                serde_json::json!({
                    "city": s.city_name,
                    "fcb_size": s.fcb_file_size,
                    "jsonl_size": s.jsonl_file_size,
                    "compression": s.compression_factor,
                    "features": s.feature_count,
                    "city_objects": s.city_object_count,
                    "vertices": s.vertex_count,
                    "vertices_per_object": s.avg_vertices_per_object,
                    "vertices_per_feature": s.avg_vertices_per_feature,
                    "attributes": s.attribute_count,
                    "semantic_attributes": s.semantic_attribute_count,
                    "avg_fcb_feature_size": s.avg_feature_size,
                    "avg_jsonl_feature_size": s.avg_cjseq_feature_size,
                    "feature_compression": s.feature_compression_factor
                })
            })
            .collect::<Vec<_>>(),
    )?;

    Ok(())
}
