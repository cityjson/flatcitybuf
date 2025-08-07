use anyhow::Result;
use anyhow::{bail, Context};
use bson::Document;
use cjseq::{CityJSON, CityJSONFeature};
use fcb_core::{FcbReader, GeometryType};
use prettytable::{Cell, Row, Table};
use serde::{Deserialize, Serialize};
use std::mem::MaybeUninit;
use std::path::Path;
use std::process::{Command, Stdio};
use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader, Write},
    time::{Duration, Instant},
};

/// Read FCB file and count geometry types
pub(crate) fn read_fcb(path: &str) -> Result<(u64, u64, u64)> {
    let input_file = File::open(path)?;
    let inputreader = BufReader::new(input_file);

    let mut reader = FcbReader::open(inputreader)?.select_all()?;
    let header = reader.header();
    let feat_count = header.features_count();
    let mut solid_count = 0;
    let mut multi_surface_count = 0;
    let mut other_count = 0;
    let mut feat_num = 0;
    while let Some(feat_buf) = reader.next()? {
        let feature = feat_buf.cur_feature();
        feature
            .objects()
            .into_iter()
            .flatten()
            .flat_map(|city_object| city_object.geometry().unwrap_or_default())
            .for_each(|geometry| match geometry.type_() {
                GeometryType::Solid => solid_count += 1,
                GeometryType::MultiSurface => multi_surface_count += 1,
                _ => other_count += 1,
            });
        feat_num += 1;
        if feat_num == feat_count {
            break;
        }
    }

    Ok((solid_count, multi_surface_count, other_count))
}

/// Read FCB file and count geometry types
#[allow(dead_code)]
pub(crate) fn read_fcb_as_cj(path: &str) -> Result<(u64, u64, u64)> {
    let input_file = File::open(path)?;
    let inputreader = BufReader::new(input_file);

    let mut reader = FcbReader::open(inputreader)?.select_all()?;
    let header = reader.header();
    let feat_count = header.features_count();
    let mut solid_count = 0;
    let mut multi_surface_count = 0;
    let mut other_count = 0;
    let mut feat_num = 0;
    while let Some(feat_buf) = reader.next()? {
        let feature = feat_buf.cur_cj_feature()?;
        feature.city_objects.iter().for_each(|(_, co)| {
            if let Some(geometries) = &co.geometry {
                for geometry in geometries {
                    match geometry.thetype {
                        cjseq::GeometryType::Solid => solid_count += 1,
                        cjseq::GeometryType::MultiSurface => multi_surface_count += 1,
                        _ => other_count += 1,
                    }
                }
            }
        });
        feat_num += 1;
        if feat_num == feat_count {
            break;
        }
    }

    Ok((solid_count, multi_surface_count, other_count))
}

/// Read CityJSONSeq file and count geometry types
fn read_cjseq(path: &str) -> Result<(u64, u64, u64)> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    let mut solid_count = 0;
    let mut multi_surface_count = 0;
    let mut other_count = 0;

    // Skip the first line (header)
    if let Some(first_line) = lines.next() {
        let _header: CityJSON = serde_json::from_str(&first_line?)?;
    }

    let mut feat_count = 0;
    // Process features one by one
    for line in lines {
        let feature: CityJSONFeature = serde_json::from_str(&line?)?;
        feat_count += 1;
        // Process each city object in this feature
        for (_id, city_object) in feature.city_objects {
            // Process geometries if they exist
            if let Some(geometries) = city_object.geometry {
                for geometry in geometries {
                    match geometry.thetype {
                        cjseq::GeometryType::Solid => solid_count += 1,
                        cjseq::GeometryType::MultiSurface => multi_surface_count += 1,
                        _ => other_count += 1,
                    }
                }
            }
        }
    }

    Ok((solid_count, multi_surface_count, other_count))
}

/// Read CBOR file and count geometry types
pub(crate) fn read_cbor(path: &str) -> Result<(u64, u64, u64)> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let value: serde_json::Value = serde_cbor::from_reader(reader)?;

    let mut solid_count = 0;
    let mut multi_surface_count = 0;
    let mut other_count = 0;

    if let Some(city_objects) = value.get("CityObjects") {
        if let Some(objects) = city_objects.as_object() {
            for (_id, obj) in objects {
                if let Some(geometries) = obj.get("geometry") {
                    if let Some(geom_array) = geometries.as_array() {
                        for geom in geom_array {
                            if let Some(type_str) = geom.get("type").and_then(|t| t.as_str()) {
                                match type_str {
                                    "Solid" => solid_count += 1,
                                    "MultiSurface" => multi_surface_count += 1,
                                    _ => other_count += 1,
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok((solid_count, multi_surface_count, other_count))
}

/// Read BSON file and count geometry types
pub(crate) fn read_bson(path: &str) -> Result<(u64, u64, u64)> {
    let mut file = File::open(path)?;
    let doc = Document::from_reader(&mut file)?;

    let mut solid_count = 0;
    let mut multi_surface_count = 0;
    let mut other_count = 0;

    if let Some(city_objects) = doc.get("CityObjects").and_then(|co| co.as_document()) {
        for (_id, obj) in city_objects {
            if let Some(geometries) = obj.as_document().and_then(|o| o.get("geometry")) {
                if let Some(geom_array) = geometries.as_array() {
                    for geom in geom_array {
                        if let Some(type_str) = geom
                            .as_document()
                            .and_then(|g| g.get("type"))
                            .and_then(|t| t.as_str())
                        {
                            match type_str {
                                "Solid" => solid_count += 1,
                                "MultiSurface" => multi_surface_count += 1,
                                _ => other_count += 1,
                            }
                        }
                    }
                }
            }
        }
    }

    Ok((solid_count, multi_surface_count, other_count))
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_read_counts_match() -> Result<()> {
        // Test all datasets with all formats
        for (dataset_name, (fcb_path, cjseq_path, cbor_path, bson_path)) in DATASETS {
            println!("Testing dataset: {}", dataset_name);

            // Define a helper to run and check each read function
            let run_test = |name: &str,
                            path: &str,
                            read_fn: fn(&str) -> Result<(u64, u64, u64)>|
             -> Result<(u64, u64, u64)> {
                println!("  Reading {} format...", name);
                let start = Instant::now();
                let result = read_fn(path)?;
                println!("  {} completed in {:.2?}", name, start.elapsed());
                Ok(result)
            };

            // Run all read functions
            let fcb_result = match run_test("FlatCityBuf", fcb_path, read_fcb) {
                Ok(res) => res,
                Err(e) => {
                    println!("  Error reading FCB: {:?}", e);
                    continue;
                }
            };

            // Test each other format against FCB
            let formats = [
                (
                    "CityJSONSeq",
                    cjseq_path,
                    read_cjseq as fn(&str) -> Result<(u64, u64, u64)>,
                ),
                (
                    "CBOR",
                    cbor_path,
                    read_cbor as fn(&str) -> Result<(u64, u64, u64)>,
                ),
                (
                    "BSON",
                    bson_path,
                    read_bson as fn(&str) -> Result<(u64, u64, u64)>,
                ),
            ];

            for (format_name, path, read_fn) in formats {
                match run_test(format_name, path, read_fn) {
                    Ok((solids, surfaces, others)) => {
                        let (fcb_solids, fcb_surfaces, fcb_others) = fcb_result;

                        // Print counts for debugging
                        println!(
                            "  {}: solids={}, surfaces={}, others={}",
                            format_name, solids, surfaces, others
                        );
                        println!(
                            "  FCB: solids={}, surfaces={}, others={}",
                            fcb_solids, fcb_surfaces, fcb_others
                        );

                        // Assert counts match
                        assert_eq!(
                            fcb_solids, solids,
                            "solid counts don't match for {} vs FCB in {}",
                            format_name, dataset_name
                        );
                        assert_eq!(
                            fcb_surfaces, surfaces,
                            "surface counts don't match for {} vs FCB in {}",
                            format_name, dataset_name
                        );
                        assert_eq!(
                            fcb_others, others,
                            "other geometry counts don't match for {} vs FCB in {}",
                            format_name, dataset_name
                        );

                        println!("  ✓ {} matches FCB", format_name);
                    }
                    Err(e) => {
                        println!("  Error reading {}: {:?}", format_name, e);
                    }
                }
            }

            println!("Completed tests for {}\n", dataset_name);
        }

        Ok(())
    }
}

const DATASETS: &[(&str, (&str, &str, &str, &str))] = &[
    (
        "3DBAG",
        (
            "benchmark_data/3DBAG.city.fcb",
            "benchmark_data/3DBAG.city.jsonl",
            "benchmark_data/3DBAG.city.cbor",
            "benchmark_data/3DBAG.city.bson",
        ),
    ),
    (
        "3DBV",
        (
            "benchmark_data/3DBV.city.fcb",
            "benchmark_data/3DBV.city.jsonl",
            "benchmark_data/3DBV.city.cbor",
            "benchmark_data/3DBV.city.bson",
        ),
    ),
    (
        "Helsinki",
        (
            "benchmark_data/Helsinki.city.fcb",
            "benchmark_data/Helsinki.city.jsonl",
            "benchmark_data/Helsinki.city.cbor",
            "benchmark_data/Helsinki.city.bson",
        ),
    ),
    (
        "Ingolstadt",
        (
            "benchmark_data/Ingolstadt.city.fcb",
            "benchmark_data/Ingolstadt.city.jsonl",
            "benchmark_data/Ingolstadt.city.cbor",
            "benchmark_data/Ingolstadt.city.bson",
        ),
    ),
    (
        "Montreal",
        (
            "benchmark_data/Montreal.city.fcb",
            "benchmark_data/Montreal.city.jsonl",
            "benchmark_data/Montreal.city.cbor",
            "benchmark_data/Montreal.city.bson",
        ),
    ),
    (
        "NYC",
        (
            "benchmark_data/NYC.fcb",
            "benchmark_data/NYC.jsonl",
            "benchmark_data/NYC.cbor",
            "benchmark_data/NYC.bson",
        ),
    ),
    (
        "Rotterdam",
        (
            "benchmark_data/Rotterdam.fcb",
            "benchmark_data/Rotterdam.jsonl",
            "benchmark_data/Rotterdam.cbor",
            "benchmark_data/Rotterdam.bson",
        ),
    ),
    (
        "Vienna",
        (
            "benchmark_data/Vienna.city.fcb",
            "benchmark_data/Vienna.city.jsonl",
            "benchmark_data/Vienna.city.cbor",
            "benchmark_data/Vienna.city.bson",
        ),
    ),
    (
        "Zurich",
        (
            "benchmark_data/Zurich.city.fcb",
            "benchmark_data/Zurich.city.jsonl",
            "benchmark_data/Zurich.city.cbor",
            "benchmark_data/Zurich.city.bson",
        ),
    ),
    (
        "Subset of Tokyo (PLATEAU)",
        (
            "benchmark_data/tokyo_plateau.fcb",
            "benchmark_data/tokyo_plateau.city.jsonl",
            "benchmark_data/tokyo_plateau.city.cbor",
            "benchmark_data/tokyo_plateau.city.bson",
        ),
    ),
    (
        "Takeshiba (PLATEAU) Building",
        (
            "benchmark_data/plateau_takeshiba_bldg.city.fcb",
            "benchmark_data/plateau_takeshiba_bldg.city.jsonl",
            "benchmark_data/plateau_takeshiba_bldg.city.cbor",
            "benchmark_data/plateau_takeshiba_bldg.city.bson",
        ),
    ),
    (
        "Takeshiba (PLATEAU) Brid",
        (
            "benchmark_data/plateau_takeshiba_brid.city.fcb",
            "benchmark_data/plateau_takeshiba_brid.city.jsonl",
            "benchmark_data/plateau_takeshiba_brid.city.cbor",
            "benchmark_data/plateau_takeshiba_brid.city.bson",
        ),
    ),
    (
        "Takeshiba (PLATEAU) Railway",
        (
            "benchmark_data/plateau_takeshiba_rwy.city.fcb",
            "benchmark_data/plateau_takeshiba_rwy.city.jsonl",
            "benchmark_data/plateau_takeshiba_rwy.city.cbor",
            "benchmark_data/plateau_takeshiba_rwy.city.bson",
        ),
    ),
    (
        "Takeshiba (PLATEAU) Transport",
        (
            "benchmark_data/plateau_takeshiba_tran.city.fcb",
            "benchmark_data/plateau_takeshiba_tran.city.jsonl",
            "benchmark_data/plateau_takeshiba_tran.city.cbor",
            "benchmark_data/plateau_takeshiba_tran.city.bson",
        ),
    ),
    (
        "Takeshiba (PLATEAU) Tunnel",
        (
            "benchmark_data/plateau_takeshiba_tun.city.fcb",
            "benchmark_data/plateau_takeshiba_tun.city.jsonl",
            "benchmark_data/plateau_takeshiba_tun.city.cbor",
            "benchmark_data/plateau_takeshiba_tun.city.bson",
        ),
    ),
    (
        "Takeshiba (PLATEAU) Vegetation",
        (
            "benchmark_data/plateau_takeshiba_bldg.city.fcb",
            "benchmark_data/plateau_takeshiba_bldg.city.jsonl",
            "benchmark_data/plateau_takeshiba_bldg.city.cbor",
            "benchmark_data/plateau_takeshiba_bldg.city.bson",
        ),
    ),
];

/// Result emitted by each child process.
#[derive(Serialize, Deserialize, Debug)]
struct Metrics {
    duration_ms: f64,
    peak_rss_bytes: u64,
    cpu_usage_percent: f64,
    iterations: u32,
    mean_duration_ms: f64,
    median_duration_ms: f64,
    std_dev_duration_ms: f64,
}

fn format_duration(d: Duration) -> String {
    if d.as_secs() > 0 {
        format!("{:.2}s", d.as_secs_f64())
    } else {
        format!("{:.2}ms", d.as_secs_f64() * 1000.0)
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.2} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

#[derive(Debug, Clone)]
struct BenchResult {
    format: String,
    duration: Duration,
    peak_memory: u64,
    cpu_usage: f64,
    iterations: u32,
    mean_duration: Duration,
    median_duration: Duration,
    std_dev_duration: Duration,
}

fn main() -> Result<()> {
    let mut args = std::env::args();
    let is_child = matches!(args.nth(1).as_deref(), Some("--child"));

    if is_child {
        // `--child <dataset-path> <format> <iterations> <warmup>`
        let dataset = args.next().context("missing dataset path")?;
        let format = args.next().context("missing format")?;
        let iterations: u32 = args
            .next()
            .context("missing iterations")?
            .parse()
            .context("invalid iterations number")?;
        let warmup: u32 = args
            .next()
            .context("missing warmup")?
            .parse()
            .context("invalid warmup number")?;
        run_child(Path::new(&dataset), &format, iterations, warmup)?;
    } else {
        coordinator()?;
    }
    Ok(())
}

// ───────────────────────────────── Coordinator ──────────────────────────────

fn coordinator() -> Result<()> {
    let mut all_results = HashMap::new();

    // Benchmark configuration
    const ITERATIONS: u32 = 50;
    const WARMUP_ITERATIONS: u32 = 5;

    println!(
        "running benchmarks with {ITERATIONS} iterations and {WARMUP_ITERATIONS} warmup iterations...\n"
    );
    println!("this may take several minutes depending on dataset sizes...\n");

    for (dataset_name, formats) in DATASETS {
        println!("processing dataset: {dataset_name}");
        let format_names = ["FlatCityBuf", "CityJSONTextSequence", "CBOR", "BSON"];

        for (i, path) in [formats.0, formats.1, formats.2, formats.3]
            .iter()
            .enumerate()
        {
            let format_name = format_names[i];
            print!(
                "  running {format_name} benchmark ({ITERATIONS} iterations + {WARMUP_ITERATIONS} warmup)... "
            );

            let start = Instant::now();
            let output = Command::new(std::env::current_exe()?)
                .args([
                    "--child",
                    *path,
                    format_name,
                    &ITERATIONS.to_string(),
                    &WARMUP_ITERATIONS.to_string(),
                ])
                .stdout(Stdio::piped())
                .spawn()?
                .wait_with_output()?;

            if !output.status.success() {
                println!("failed!");
                eprintln!(
                    "child failed for {dataset_name} / {format_name}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                continue;
            }

            let m: Metrics = match serde_json::from_slice(&output.stdout) {
                Ok(metrics) => metrics,
                Err(e) => {
                    println!("failed to parse results!");
                    eprintln!("failed to parse child JSON: {e}");
                    continue;
                }
            };

            let duration = Duration::from_secs_f64(m.duration_ms / 1000.0);
            let mean_duration = Duration::from_secs_f64(m.mean_duration_ms / 1000.0);
            let median_duration = Duration::from_secs_f64(m.median_duration_ms / 1000.0);
            let std_dev_duration = Duration::from_secs_f64(m.std_dev_duration_ms / 1000.0);

            let result = BenchResult {
                format: format_name.to_string(),
                duration,
                peak_memory: m.peak_rss_bytes,
                cpu_usage: m.cpu_usage_percent,
                iterations: m.iterations,
                mean_duration,
                median_duration,
                std_dev_duration,
            };

            all_results.insert(format!("{dataset_name}_{format_name}"), result);
            println!(
                "completed in {} (mean: {}, median: {}, std dev: {})",
                format_duration(duration),
                format_duration(mean_duration),
                format_duration(median_duration),
                format_duration(std_dev_duration)
            );
        }
        println!();
    }

    // Display comprehensive results
    print_benchmark_results(&all_results);
    Ok(())
}

/// Print comprehensive benchmark results to standard output
fn print_benchmark_results(results: &HashMap<String, BenchResult>) {
    println!("\n{:=<100}", "");
    println!("COMPREHENSIVE BENCHMARK RESULTS WITH STATISTICS");
    println!("{:=<100}", "");

    // Main results table with statistics
    println!(
        "\nAll Results (with {} iterations each):",
        results.values().next().map(|r| r.iterations).unwrap_or(0)
    );
    let mut summary_table = Table::new();
    summary_table.add_row(Row::new(vec![
        Cell::new("Dataset"),
        Cell::new("Format"),
        Cell::new("Mean Time"),
        Cell::new("Median Time"),
        Cell::new("Std Dev"),
        Cell::new("Memory"),
        Cell::new("CPU %"),
    ]));

    for (dataset_name, _) in DATASETS {
        let format_names = ["FlatCityBuf", "CityJSONTextSequence", "CBOR", "BSON"];

        for format_name in &format_names {
            let key = format!("{dataset_name}_{format_name}");
            if let Some(result) = results.get(&key) {
                summary_table.add_row(Row::new(vec![
                    Cell::new(dataset_name),
                    Cell::new(&result.format),
                    Cell::new(&format_duration(result.mean_duration)),
                    Cell::new(&format_duration(result.median_duration)),
                    Cell::new(&format_duration(result.std_dev_duration)),
                    Cell::new(&format_bytes(result.peak_memory)),
                    Cell::new(&format!("{:.2}%", result.cpu_usage)),
                ]));
            }
        }
    }
    summary_table.printstd();

    // Comparison tables for each format vs FlatCityBuf (using mean times)
    let formats_to_compare = ["CityJSONTextSequence", "CBOR", "BSON"];

    for format in formats_to_compare {
        println!("\n{format} vs FlatCityBuf Comparison (Mean Times):");
        let mut comparison_table = Table::new();

        // Header row
        comparison_table.add_row(Row::new(vec![
            Cell::new("Dataset"),
            Cell::new(&format!("{format} Mean")),
            Cell::new("FCB Mean"),
            Cell::new("Time Ratio"),
            Cell::new(&format!("{format} Std Dev")),
            Cell::new("FCB Std Dev"),
            Cell::new(&format!("{format} Memory")),
            Cell::new("FCB Memory"),
            Cell::new("Memory Ratio"),
        ]));

        for (dataset_name, _) in DATASETS {
            let fcb_key = format!("{dataset_name}_FlatCityBuf");
            let format_key = format!("{dataset_name}_{format}");

            if let (Some(fcb_result), Some(format_result)) =
                (results.get(&fcb_key), results.get(&format_key))
            {
                println!("fcb_result: {fcb_result:?}");
                // Calculate ratios using mean times
                let time_ratio = format_result.mean_duration.as_secs_f64()
                    / fcb_result.mean_duration.as_secs_f64();

                let memory_ratio = format_result.peak_memory as f64 / fcb_result.peak_memory as f64;

                comparison_table.add_row(Row::new(vec![
                    Cell::new(dataset_name),
                    Cell::new(&format_duration(format_result.mean_duration)),
                    Cell::new(&format_duration(fcb_result.mean_duration)),
                    Cell::new(&format!("{time_ratio:.2}x")),
                    Cell::new(&format_duration(format_result.std_dev_duration)),
                    Cell::new(&format_duration(fcb_result.std_dev_duration)),
                    Cell::new(&format_bytes(format_result.peak_memory)),
                    Cell::new(&format_bytes(fcb_result.peak_memory)),
                    Cell::new(&format!("{memory_ratio:.2}x")),
                ]));
            }
        }

        comparison_table.printstd();
    }

    // Summary table showing best performer per metric (using mean values)
    println!("\nSummary - Best Format Per Metric (Mean Values):");
    let mut best_format_table = Table::new();
    best_format_table.add_row(Row::new(vec![
        Cell::new("Dataset"),
        Cell::new("Fastest (Mean)"),
        Cell::new("Most Consistent"),
        Cell::new("Lowest Memory"),
        Cell::new("Lowest CPU"),
    ]));

    for (dataset_name, _) in DATASETS {
        let format_names = ["FlatCityBuf", "CityJSONTextSequence", "CBOR", "BSON"];
        let mut fastest = ("None", Duration::from_secs(u64::MAX));
        let mut most_consistent = ("None", Duration::from_secs(u64::MAX));
        let mut lowest_memory = ("None", u64::MAX);
        let mut lowest_cpu = ("None", f64::MAX);

        for format_name in &format_names {
            let key = format!("{dataset_name}_{format_name}");
            if let Some(result) = results.get(&key) {
                if result.mean_duration < fastest.1 {
                    fastest = (&result.format, result.mean_duration);
                }
                if result.std_dev_duration < most_consistent.1 {
                    most_consistent = (&result.format, result.std_dev_duration);
                }
                if result.peak_memory < lowest_memory.1 {
                    lowest_memory = (&result.format, result.peak_memory);
                }
                if result.cpu_usage < lowest_cpu.1 {
                    lowest_cpu = (&result.format, result.cpu_usage);
                }
            }
        }

        best_format_table.add_row(Row::new(vec![
            Cell::new(dataset_name),
            Cell::new(fastest.0),
            Cell::new(most_consistent.0),
            Cell::new(lowest_memory.0),
            Cell::new(lowest_cpu.0),
        ]));
    }
    best_format_table.printstd();

    // Export results to files
    export_results_to_csv(results);

    println!("\n{:=<100}", "");
    println!("BENCHMARK COMPLETED");
    println!("{:=<100}", "");
}

/// Export benchmark raw data to CSV file for further analysis
fn export_results_to_csv(results: &HashMap<String, BenchResult>) {
    use std::io::Write;

    // Export main results CSV with statistics
    let filename = "benchmark_results.csv";
    match File::create(filename) {
        Ok(mut file) => {
            if let Err(e) = writeln!(
                file,
                "Dataset,Format,Iterations,MeanTimeMs,MedianTimeMs,StdDevTimeMs,MemoryBytes,CpuPercent"
            ) {
                eprintln!("error writing CSV header: {e:?}");
                return;
            }

            for (dataset_name, _) in DATASETS {
                let format_names = ["FlatCityBuf", "CityJSONTextSequence", "CBOR", "BSON"];
                for format_name in &format_names {
                    let key = format!("{dataset_name}_{format_name}");
                    if let Some(result) = results.get(&key) {
                        let mean_time_ms = result.mean_duration.as_secs_f64() * 1000.0;
                        let median_time_ms = result.median_duration.as_secs_f64() * 1000.0;
                        let std_dev_time_ms = result.std_dev_duration.as_secs_f64() * 1000.0;
                        if let Err(e) = writeln!(
                            file,
                            "{},{},{},{},{},{},{},{}",
                            dataset_name,
                            result.format,
                            result.iterations,
                            mean_time_ms,
                            median_time_ms,
                            std_dev_time_ms,
                            result.peak_memory,
                            result.cpu_usage
                        ) {
                            eprintln!("error writing CSV row: {e:?}");
                            return;
                        }
                    }
                }
            }
            println!("benchmark raw data with statistics saved to: {filename}");
        }
        Err(e) => {
            eprintln!("error creating CSV file: {e:?}");
        }
    }

    // Export detailed comparison tables
    export_comparison_tables(results);
}

/// Export detailed comparison tables to text and CSV files
fn export_comparison_tables(results: &HashMap<String, BenchResult>) {
    use chrono::Local;
    use std::io::Write;

    let now = Local::now();
    let timestamp = now.format("%Y%m%d_%H%M%S");
    let iterations = results.values().next().map(|r| r.iterations).unwrap_or(0);

    // Export comprehensive text report
    let text_filename = format!("benchmark_results_{timestamp}.txt");
    match File::create(&text_filename) {
        Ok(mut file) => {
            writeln!(file, "FLATCITYBUF BENCHMARK RESULTS WITH STATISTICS").unwrap();
            writeln!(file, "Generated: {}", now.format("%Y-%m-%d %H:%M:%S")).unwrap();
            writeln!(file, "Iterations per test: {iterations}").unwrap();
            writeln!(file, "{:=<120}", "").unwrap();

            // Main results table
            writeln!(file, "\nALL RESULTS:").unwrap();
            writeln!(file, "{:-<120}", "").unwrap();
            writeln!(
                file,
                "{:<20} {:<20} {:>12} {:>12} {:>12} {:>15} {:>10}",
                "Dataset", "Format", "Mean Time", "Median Time", "Std Dev", "Memory", "CPU %"
            )
            .unwrap();
            writeln!(file, "{:-<120}", "").unwrap();

            for (dataset_name, _) in DATASETS {
                let format_names = ["FlatCityBuf", "CityJSONTextSequence", "CBOR", "BSON"];
                for format_name in &format_names {
                    let key = format!("{dataset_name}_{format_name}");
                    if let Some(result) = results.get(&key) {
                        writeln!(
                            file,
                            "{:<20} {:<20} {:>12} {:>12} {:>12} {:>15} {:>9.2}%",
                            dataset_name,
                            result.format,
                            format_duration(result.mean_duration),
                            format_duration(result.median_duration),
                            format_duration(result.std_dev_duration),
                            format_bytes(result.peak_memory),
                            result.cpu_usage
                        )
                        .unwrap();
                    }
                }
            }

            // Detailed comparison tables
            let formats_to_compare = ["CityJSONTextSequence", "CBOR", "BSON"];
            for format in formats_to_compare {
                writeln!(file, "\n{format} vs FlatCityBuf Comparison (Mean Times):").unwrap();
                writeln!(file, "{:-<140}", "").unwrap();
                writeln!(
                    file,
                    "{:<20} {:>12} {:>12} {:>10} {:>12} {:>12} {:>12} {:>12} {:>10}",
                    "Dataset",
                    format!("{} Mean", format),
                    "FCB Mean",
                    "Time Ratio",
                    format!("{} StdDev", format),
                    "FCB StdDev",
                    format!("{} Mem", format),
                    "FCB Memory",
                    "Mem Ratio"
                )
                .unwrap();
                writeln!(file, "{:-<140}", "").unwrap();

                for (dataset_name, _) in DATASETS {
                    let fcb_key = format!("{dataset_name}_FlatCityBuf");
                    let format_key = format!("{dataset_name}_{format}");

                    if let (Some(fcb_result), Some(format_result)) =
                        (results.get(&fcb_key), results.get(&format_key))
                    {
                        let time_ratio = format_result.mean_duration.as_secs_f64()
                            / fcb_result.mean_duration.as_secs_f64();
                        let memory_ratio =
                            format_result.peak_memory as f64 / fcb_result.peak_memory as f64;

                        writeln!(
                            file,
                            "{:<20} {:>12} {:>12} {:>9.2}x {:>12} {:>12} {:>12} {:>12} {:>9.2}x",
                            dataset_name,
                            format_duration(format_result.mean_duration),
                            format_duration(fcb_result.mean_duration),
                            time_ratio,
                            format_duration(format_result.std_dev_duration),
                            format_duration(fcb_result.std_dev_duration),
                            format_bytes(format_result.peak_memory),
                            format_bytes(fcb_result.peak_memory),
                            memory_ratio
                        )
                        .unwrap();
                    }
                }
            }

            println!("comprehensive benchmark report saved to: {text_filename}");
        }
        Err(e) => {
            eprintln!("error creating text report: {e:?}");
        }
    }

    // Export comparison CSV files with statistics
    let formats_to_compare = ["CityJSONTextSequence", "CBOR", "BSON"];
    for format in formats_to_compare {
        let csv_filename = format!("comparison_{}_{}.csv", format.to_lowercase(), timestamp);
        match File::create(&csv_filename) {
            Ok(mut file) => {
                // Write detailed comparison CSV header with statistics
                writeln!(file, "Dataset,{format}_Mean_Ms,FCB_Mean_Ms,Time_Ratio,{format}_Median_Ms,FCB_Median_Ms,{format}_StdDev_Ms,FCB_StdDev_Ms,{format}_Memory_Bytes,FCB_Memory_Bytes,Memory_Ratio,{format}_CPU_Percent,FCB_CPU_Percent").unwrap();

                for (dataset_name, _) in DATASETS {
                    let fcb_key = format!("{dataset_name}_FlatCityBuf");
                    let format_key = format!("{dataset_name}_{format}");

                    if let (Some(fcb_result), Some(format_result)) =
                        (results.get(&fcb_key), results.get(&format_key))
                    {
                        let time_ratio = format_result.mean_duration.as_secs_f64()
                            / fcb_result.mean_duration.as_secs_f64();
                        let memory_ratio =
                            format_result.peak_memory as f64 / fcb_result.peak_memory as f64;

                        writeln!(
                            file,
                            "{},{},{},{:.3},{},{},{},{},{},{},{:.3},{:.2},{:.2}",
                            dataset_name,
                            format_result.mean_duration.as_secs_f64() * 1000.0,
                            fcb_result.mean_duration.as_secs_f64() * 1000.0,
                            time_ratio,
                            format_result.median_duration.as_secs_f64() * 1000.0,
                            fcb_result.median_duration.as_secs_f64() * 1000.0,
                            format_result.std_dev_duration.as_secs_f64() * 1000.0,
                            fcb_result.std_dev_duration.as_secs_f64() * 1000.0,
                            format_result.peak_memory,
                            fcb_result.peak_memory,
                            memory_ratio,
                            format_result.cpu_usage,
                            fcb_result.cpu_usage
                        )
                        .unwrap();
                    }
                }
                println!("comparison CSV with statistics saved to: {csv_filename}");
            }
            Err(e) => {
                eprintln!("error creating comparison CSV {csv_filename}: {e:?}");
            }
        }
    }

    // Export summary CSV with statistics
    let summary_filename = format!("benchmark_summary_{timestamp}.csv");
    match File::create(&summary_filename) {
        Ok(mut file) => {
            writeln!(file, "Dataset,Fastest_Format,Fastest_Mean_Ms,Most_Consistent_Format,Lowest_StdDev_Ms,Lowest_Memory_Format,Lowest_Memory_Bytes,Lowest_CPU_Format,Lowest_CPU_Percent").unwrap();

            for (dataset_name, _) in DATASETS {
                let format_names = ["FlatCityBuf", "CityJSONTextSequence", "CBOR", "BSON"];
                let mut fastest = ("None", Duration::from_secs(u64::MAX));
                let mut most_consistent = ("None", Duration::from_secs(u64::MAX));
                let mut lowest_memory = ("None", u64::MAX);
                let mut lowest_cpu = ("None", f64::MAX);

                for format_name in &format_names {
                    let key = format!("{dataset_name}_{format_name}");
                    if let Some(result) = results.get(&key) {
                        if result.mean_duration < fastest.1 {
                            fastest = (&result.format, result.mean_duration);
                        }
                        if result.std_dev_duration < most_consistent.1 {
                            most_consistent = (&result.format, result.std_dev_duration);
                        }
                        if result.peak_memory < lowest_memory.1 {
                            lowest_memory = (&result.format, result.peak_memory);
                        }
                        if result.cpu_usage < lowest_cpu.1 {
                            lowest_cpu = (&result.format, result.cpu_usage);
                        }
                    }
                }

                writeln!(
                    file,
                    "{},{},{},{},{},{},{},{:.2},{:.2}",
                    dataset_name,
                    fastest.0,
                    fastest.1.as_secs_f64() * 1000.0,
                    most_consistent.0,
                    most_consistent.1.as_secs_f64() * 1000.0,
                    lowest_memory.0,
                    lowest_memory.1,
                    lowest_cpu.0,
                    lowest_cpu.1
                )
                .unwrap();
            }
            println!("benchmark summary with statistics saved to: {summary_filename}");
        }
        Err(e) => {
            eprintln!("error creating summary CSV: {e:?}");
        }
    }
}

// ───────────────────────────────── Child process ────────────────────────────

fn run_child(dataset: &Path, format: &str, iterations: u32, warmup: u32) -> Result<()> {
    let path_str = dataset.to_str().context("invalid path")?;

    // Warm-up iterations (not measured)
    for i in 0..warmup {
        if i == 0 {
            eprintln!("performing {warmup} warm-up iterations...");
        }
        read_dataset_internal(path_str, format)?;
    }

    eprintln!("starting {iterations} measured iterations...");

    let mut durations = Vec::with_capacity(iterations as usize);
    let mut peak_rss_values = Vec::with_capacity(iterations as usize);
    let mut cpu_usage_values = Vec::with_capacity(iterations as usize);

    let overall_start = Instant::now();
    let overall_usage_before = rusage();

    // Measured iterations
    for i in 0..iterations {
        if i % 10 == 0 && i > 0 {
            eprintln!("completed {i}/{iterations} iterations");
        }

        let usage_before = rusage();
        let start = Instant::now();

        // Perform the actual read
        read_dataset_internal(path_str, format).with_context(|| {
            format!(
                "while reading dataset '{}' using format '{}' (iteration {})",
                dataset.display(),
                format,
                i + 1
            )
        })?;

        let dur = start.elapsed();
        let usage_after = rusage();
        let peak_rss = platform_rss_bytes(&usage_after);

        let cpu_user =
            timeval_to_secs(usage_after.ru_utime) - timeval_to_secs(usage_before.ru_utime);
        let cpu_sys =
            timeval_to_secs(usage_after.ru_stime) - timeval_to_secs(usage_before.ru_stime);
        let cpu_time = cpu_user + cpu_sys;
        let cpu_pct = cpu_time / dur.as_secs_f64() * 100.0;

        durations.push(dur);
        peak_rss_values.push(peak_rss);
        cpu_usage_values.push(cpu_pct);
    }

    let overall_dur = overall_start.elapsed();
    let overall_usage_after = rusage();
    let overall_peak_rss = platform_rss_bytes(&overall_usage_after);

    let overall_cpu_user = timeval_to_secs(overall_usage_after.ru_utime)
        - timeval_to_secs(overall_usage_before.ru_utime);
    let overall_cpu_sys = timeval_to_secs(overall_usage_after.ru_stime)
        - timeval_to_secs(overall_usage_before.ru_stime);
    let overall_cpu_time = overall_cpu_user + overall_cpu_sys;
    let overall_cpu_pct = overall_cpu_time / overall_dur.as_secs_f64() * 100.0;

    // Calculate statistics
    let mean_duration = calculate_mean(&durations);
    let median_duration = calculate_median(&mut durations.clone());
    let std_dev_duration = calculate_std_dev(&durations, mean_duration);

    let mean_peak_rss = peak_rss_values.iter().sum::<u64>() / peak_rss_values.len() as u64;
    let mean_cpu_usage = cpu_usage_values.iter().sum::<f64>() / cpu_usage_values.len() as f64;

    eprintln!("benchmark completed: {iterations} iterations");
    eprintln!(
        "mean duration: {:.2}ms, median: {:.2}ms, std dev: {:.2}ms",
        mean_duration.as_secs_f64() * 1000.0,
        median_duration.as_secs_f64() * 1000.0,
        std_dev_duration.as_secs_f64() * 1000.0
    );

    let result = Metrics {
        duration_ms: overall_dur.as_secs_f64() * 1e3,
        peak_rss_bytes: overall_peak_rss.max(mean_peak_rss), // Use the higher of overall or mean
        cpu_usage_percent: overall_cpu_pct,
        iterations,
        mean_duration_ms: mean_duration.as_secs_f64() * 1e3,
        median_duration_ms: median_duration.as_secs_f64() * 1e3,
        std_dev_duration_ms: std_dev_duration.as_secs_f64() * 1e3,
    };
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}

// Statistical calculation functions
fn calculate_mean(durations: &[Duration]) -> Duration {
    let total_nanos: u128 = durations.iter().map(|d| d.as_nanos()).sum();
    Duration::from_nanos((total_nanos / durations.len() as u128) as u64)
}

fn calculate_median(durations: &mut [Duration]) -> Duration {
    durations.sort();
    let len = durations.len();
    if len % 2 == 0 {
        let mid1 = durations[len / 2 - 1].as_nanos();
        let mid2 = durations[len / 2].as_nanos();
        Duration::from_nanos(((mid1 + mid2) / 2) as u64)
    } else {
        durations[len / 2]
    }
}

fn calculate_std_dev(durations: &[Duration], mean: Duration) -> Duration {
    let mean_nanos = mean.as_nanos() as f64;
    let variance: f64 = durations
        .iter()
        .map(|d| {
            let diff = d.as_nanos() as f64 - mean_nanos;
            diff * diff
        })
        .sum::<f64>()
        / durations.len() as f64;

    Duration::from_nanos(variance.sqrt() as u64)
}

// ───────────────────────────── Platform helpers ─────────────────────────────

#[cfg(target_os = "linux")]
fn platform_rss_bytes(ru: &libc::rusage) -> u64 {
    ru.ru_maxrss as u64 * 1024 // ru_maxrss is kB on Linux
}
#[cfg(not(target_os = "linux"))]
fn platform_rss_bytes(ru: &libc::rusage) -> u64 {
    ru.ru_maxrss as u64 // already bytes on macOS / *BSD
}

fn timeval_to_secs(tv: libc::timeval) -> f64 {
    tv.tv_sec as f64 + tv.tv_usec as f64 / 1_000_000.0
}

fn rusage() -> libc::rusage {
    unsafe {
        let mut ru = MaybeUninit::<libc::rusage>::uninit();
        libc::getrusage(libc::RUSAGE_SELF, ru.as_mut_ptr());
        ru.assume_init()
    }
}

fn read_dataset_internal(path: &str, format: &str) -> Result<()> {
    match format {
        "FlatCityBuf" => {
            read_fcb(path)?;
            Ok(())
        }
        "CityJSONTextSequence" => {
            read_cjseq(path)?;
            Ok(())
        }
        "CBOR" => {
            read_cbor(path)?;
            Ok(())
        }
        "BSON" => {
            read_bson(path)?;
            Ok(())
        }
        _ => bail!("unknown format: {format}"),
    }
}
