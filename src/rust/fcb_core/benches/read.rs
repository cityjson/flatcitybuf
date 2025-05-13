use anyhow::Result;
use bson::Document;
use cjseq::{CityJSON, CityJSONFeature};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use fcb_core::{FcbReader, GeometryType};
use prettytable::{Cell, Row, Table};
use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
    time::{Duration, Instant},
};
use sysinfo::{Pid, System};

// Enable heap profiling with dhat
#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

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
    use std::time::Instant;

    use anyhow::Result;

    use crate::{read_bson, read_cbor, read_cjseq, read_fcb, DATASETS};
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
        "Takeshiba (PLATEAU) Brid",
        (
            "benchmark_data/plateau_takeshiba_brid.fcb",
            "benchmark_data/plateau_takeshiba_brid.city.jsonl",
            "benchmark_data/plateau_takeshiba_brid.city.cbor",
            "benchmark_data/plateau_takeshiba_brid.city.bson",
        ),
    ),
    (
        "Takeshiba (PLATEAU) Rail way",
        (
            "benchmark_data/plateau_takeshiba_rwy.fcb",
            "benchmark_data/plateau_takeshiba_rwy.city.jsonl",
            "benchmark_data/plateau_takeshiba_rwy.city.cbor",
            "benchmark_data/plateau_takeshiba_rwy.city.bson",
        ),
    ),
    (
        "Takeshiba (PLATEAU) Transport",
        (
            "benchmark_data/plateau_takeshiba_tran.fcb",
            "benchmark_data/plateau_takeshiba_tran.city.jsonl",
            "benchmark_data/plateau_takeshiba_tran.city.cbor",
            "benchmark_data/plateau_takeshiba_tran.city.bson",
        ),
    ),
    (
        "Takeshiba (PLATEAU) Tunnel",
        (
            "benchmark_data/plateau_takeshiba_tun.fcb",
            "benchmark_data/plateau_takeshiba_tun.city.jsonl",
            "benchmark_data/plateau_takeshiba_tun.city.cbor",
            "benchmark_data/plateau_takeshiba_tun.city.bson",
        ),
    ),
    (
        "Takeshiba (PLATEAU) Vegetation",
        (
            "benchmark_data/plateau_takeshiba_bldg.fcb",
            "benchmark_data/plateau_takeshiba_bldg.city.jsonl",
            "benchmark_data/plateau_takeshiba_bldg.city.cbor",
            "benchmark_data/plateau_takeshiba_bldg.city.bson",
        ),
    ),
];

fn format_duration(d: Duration) -> String {
    if d.as_secs() > 0 {
        format!("{:.2}s", d.as_secs_f64())
    } else {
        format!("{:.2}ms", d.as_millis() as f64)
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.2} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

#[derive(Debug)]
struct BenchResult {
    format: String,
    duration: Duration,
    peak_memory: u64,
    cpu_usage: f32,
}

/// Benchmark a read function with comprehensive metrics
fn benchmark_read_fn<F>(
    iterations: u32,
    format_name: &str,
    path: &str,
    read_fn: F,
) -> Result<BenchResult>
where
    F: Fn(&str) -> Result<(u64, u64, u64)>,
{
    let start = Instant::now();
    let mut total_duration = Duration::new(0, 0);
    let mut peak_memory: u64 = 0;
    let mut cpu_usage_sum: f32 = 0.0;

    let process_id = std::process::id();
    let pid = Pid::from_u32(process_id);
    let mut sys = System::new();
    sys.refresh_all();

    let process = match sys.process(pid) {
        Some(p) => p,
        None => return Err(anyhow::anyhow!("failed to get process info")),
    };

    // Initial memory state
    let initial_memory = process.memory();

    // Optional: enable dhat profiling for heap allocations
    #[cfg(feature = "dhat-heap")]
    let _profiler = if format_name == "FlatCityBuf" && iterations == 1 {
        println!("starting dhat heap profiling for {}", path);
        Some(dhat::Profiler::new_heap())
    } else {
        None
    };

    for i in 0..iterations {
        // Refresh system info
        sys.refresh_all();

        // Get the process again
        let process = match sys.process(pid) {
            Some(p) => p,
            None => return Err(anyhow::anyhow!("failed to get process info")),
        };

        // Record CPU usage before the iteration
        let cpu_before = process.cpu_usage();

        // Record memory before the iteration
        let mem_stats_before = memory_stats::memory_stats()
            .ok_or_else(|| anyhow::anyhow!("failed to get memory stats"))?;

        // Execute the read function and measure time
        let iter_start = Instant::now();
        let _ = read_fn(black_box(path))?;
        let iter_duration = iter_start.elapsed();
        total_duration += iter_duration;

        // Wait a moment to get stable CPU measurements
        std::thread::sleep(Duration::from_millis(10));

        // Refresh system info after the iteration
        sys.refresh_all();

        // Get the process again
        let process = match sys.process(pid) {
            Some(p) => p,
            None => return Err(anyhow::anyhow!("failed to get process info")),
        };

        // Record CPU usage after the iteration
        let cpu_after = process.cpu_usage();
        let cpu_delta = cpu_after - cpu_before;
        cpu_usage_sum += cpu_delta;

        // Record memory after the iteration
        let mem_stats_after = memory_stats::memory_stats()
            .ok_or_else(|| anyhow::anyhow!("failed to get memory stats"))?;
        let current_memory = mem_stats_after.physical_mem;
        peak_memory = peak_memory.max(current_memory as u64);

        // Optional progress reporting
        if iterations > 1 && (i + 1) % (iterations / 10).max(1) == 0 {
            println!(
                "progress: {}/{} iterations for {} - {}",
                i + 1,
                iterations,
                format_name,
                path
            );
        }
    }

    // Final process memory usage (subtract initial memory to get delta)
    sys.refresh_all();
    let final_memory = match sys.process(pid) {
        Some(p) => p.memory(),
        None => initial_memory,
    };
    let memory_delta = final_memory.saturating_sub(initial_memory);

    // Calculate averages
    let avg_duration = if iterations > 0 {
        total_duration / iterations
    } else {
        Duration::new(0, 0)
    };

    let avg_cpu_usage = if iterations > 0 {
        cpu_usage_sum / iterations as f32
    } else {
        0.0
    };

    let total_elapsed = start.elapsed();

    Ok(BenchResult {
        format: format_name.to_string(),
        duration: avg_duration,
        peak_memory,
        cpu_usage: avg_cpu_usage,
    })
}

pub fn read_benchmark(c: &mut Criterion) {
    // Optional: Initialize dhat profiler if the feature is enabled
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::builder().testing().build();

    let mut group = c.benchmark_group("read");

    let iterations: u32 = 1;
    // Increase warm-up time and measurement time to prevent timeouts
    group
        .sample_size(iterations as usize)
        .warm_up_time(Duration::from_secs(2));

    let mut results = HashMap::new();

    // Print table headers for real-time results
    println!("\nBenchmark Results (Real-time):");
    println!(
        "{:<15} {:<20} {:<15} {:<15} {:<15}",
        "Dataset", "Format", "Mean Time", "Peak Memory", "CPU Usage"
    );
    println!("{:-<80}", "");

    for (size, (fcb_path, cjseq_path, cbor_path, bson_path)) in DATASETS {
        // FCB benchmark
        println!("benchmarking FlatCityBuf for dataset: {}", size);
        let result = benchmark_read_fn(iterations, "FlatCityBuf", fcb_path, read_fcb)
            .unwrap_or_else(|e| {
                println!("error in fcb benchmark: {:?}", e);
                BenchResult {
                    format: "FlatCityBuf".to_string(),
                    duration: Duration::new(0, 0),
                    peak_memory: 0,
                    cpu_usage: 0.0,
                }
            });

        // Print real-time result
        println!(
            "{:<15} {:<20} {:<15} {:<15} {:.2}%",
            size,
            result.format,
            format_duration(result.duration),
            format_bytes(result.peak_memory),
            result.cpu_usage
        );

        group.bench_with_input(
            BenchmarkId::new("FlatCityBuf", size),
            fcb_path,
            |b, path| b.iter(|| read_fcb(black_box(path))),
        );

        results.insert(format!("{}_fcb", size), result);

        // CJSeq benchmark
        println!("benchmarking CityJSONTextSequence for dataset: {}", size);
        let result = benchmark_read_fn(iterations, "CityJSONTextSequence", cjseq_path, read_cjseq)
            .unwrap_or_else(|e| {
                println!("error in cjseq benchmark: {:?}", e);
                BenchResult {
                    format: "CityJSONTextSequence".to_string(),
                    duration: Duration::new(0, 0),
                    peak_memory: 0,
                    cpu_usage: 0.0,
                }
            });

        // Print real-time result
        println!(
            "{:<15} {:<20} {:<15} {:<15} {:.2}%",
            size,
            result.format,
            format_duration(result.duration),
            format_bytes(result.peak_memory),
            result.cpu_usage
        );

        group.bench_with_input(
            BenchmarkId::new("CityJSONTextSequence", size),
            cjseq_path,
            |b, path| b.iter(|| read_cjseq(black_box(path))),
        );

        results.insert(format!("{}_cjseq", size), result);

        // CBOR benchmark
        println!("benchmarking CBOR for dataset: {}", size);
        let result =
            benchmark_read_fn(iterations, "CBOR", cbor_path, read_cbor).unwrap_or_else(|e| {
                println!("error in cbor benchmark: {:?}", e);
                BenchResult {
                    format: "CBOR".to_string(),
                    duration: Duration::new(0, 0),
                    peak_memory: 0,
                    cpu_usage: 0.0,
                }
            });

        // Print real-time result
        println!(
            "{:<15} {:<20} {:<15} {:<15} {:.2}%",
            size,
            result.format,
            format_duration(result.duration),
            format_bytes(result.peak_memory),
            result.cpu_usage
        );

        group.bench_with_input(BenchmarkId::new("CBOR", size), cbor_path, |b, path| {
            b.iter(|| read_cbor(black_box(path)))
        });

        results.insert(format!("{}_cbor", size), result);

        // BSON benchmark
        println!("benchmarking BSON for dataset: {}", size);
        let result =
            benchmark_read_fn(iterations, "BSON", bson_path, read_bson).unwrap_or_else(|e| {
                println!("error in bson benchmark: {:?}", e);
                BenchResult {
                    format: "BSON".to_string(),
                    duration: Duration::new(0, 0),
                    peak_memory: 0,
                    cpu_usage: 0.0,
                }
            });

        // Print real-time result
        println!(
            "{:<15} {:<20} {:<15} {:<15} {:.2}%",
            size,
            result.format,
            format_duration(result.duration),
            format_bytes(result.peak_memory),
            result.cpu_usage
        );

        group.bench_with_input(BenchmarkId::new("BSON", size), bson_path, |b, path| {
            b.iter(|| read_bson(black_box(path)))
        });

        results.insert(format!("{}_bson", size), result);

        // Add a separator between datasets
        println!("{:-<80}", "");
    }

    group.finish();

    // Print comprehensive results summary table
    print_benchmark_results(&results);
}

/// Print comprehensive benchmark results to standard output
fn print_benchmark_results(results: &HashMap<String, BenchResult>) {
    println!("\nComprehensive Benchmark Results:");
    let mut summary_table = Table::new();
    // Add header row
    summary_table.add_row(Row::new(vec![
        Cell::new("Dataset"),
        Cell::new("Format"),
        Cell::new("Mean Time"),
        Cell::new("Peak Memory"),
        Cell::new("CPU Usage"),
    ]));
    // println!("{:-<75}", "");

    for (size, _) in DATASETS {
        for format in &["fcb", "cjseq", "cbor", "bson"] {
            if let Some(result) = results.get(&format!("{}_{}", size, format)) {
                summary_table.add_row(Row::new(vec![
                    Cell::new(size),
                    Cell::new(&result.format),
                    Cell::new(&format_duration(result.duration)),
                    Cell::new(&format_bytes(result.peak_memory)),
                    Cell::new(&format!("{:.2}%", result.cpu_usage)),
                ]));
            }
        }
        // Add a separator between datasets
        println!("{:-<75}", "");
    }
    summary_table.printstd();

    // Summary table - best performance per metric
    println!("\nSummary - Best Format Per Metric:");
    summary_table.add_row(Row::new(vec![
        Cell::new("Dataset"),
        Cell::new("Fastest"),
        Cell::new("Lowest Memory"),
        Cell::new("Lowest CPU"),
    ]));
    // println!("{:-<60}", "");

    for (size, _) in DATASETS {
        let formats = ["fcb", "cjseq", "cbor", "bson"];
        let mut fastest = ("None", Duration::from_secs(u64::MAX));
        let mut lowest_memory = ("None", u64::MAX);
        let mut lowest_cpu = ("None", f32::MAX);

        for format in &formats {
            if let Some(result) = results.get(&format!("{}_{}", size, format)) {
                if result.duration < fastest.1 {
                    fastest = (&result.format, result.duration);
                }
                if result.peak_memory < lowest_memory.1 {
                    lowest_memory = (&result.format, result.peak_memory);
                }
                if result.cpu_usage < lowest_cpu.1 {
                    lowest_cpu = (&result.format, result.cpu_usage);
                }
            }
        }

        summary_table.add_row(Row::new(vec![
            Cell::new(size),
            Cell::new(fastest.0),
            Cell::new(lowest_memory.0),
            Cell::new(lowest_cpu.0),
        ]));
        // println!(
        //     "{:<15} {:<15} {:<15} {:<15}",
        //     size, fastest.0, lowest_memory.0, lowest_cpu.0
        // );
    }
    summary_table.printstd();
}

/// Add a feature flag to enable dhat profiling
#[cfg(feature = "dhat-heap")]
fn heap_profile() {
    use dhat::Profiler;
    // Initialize the profiler
    let _profiler = Profiler::new_heap();

    // Run just one iteration of each format for profiling
    println!("Running heap profiling for FlatCityBuf");
    let _ = read_fcb("benchmark_data/3DBAG.city.fcb");

    println!("Running heap profiling for CityJSONTextSequence");
    let _ = read_cjseq("benchmark_data/3DBAG.city.jsonl");

    println!("Running heap profiling for CBOR");
    let _ = read_cbor("benchmark_data/3DBAG.city.cbor");

    println!("Running heap profiling for BSON");
    let _ = read_bson("benchmark_data/3DBAG.city.bson");
}

// Define the benchmark group
#[cfg(not(feature = "dhat-heap"))]
criterion_group!(benches, read_benchmark);

// Use a different configuration when heap profiling is enabled
#[cfg(feature = "dhat-heap")]
criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(1);
    targets = read_benchmark
}

criterion_main!(benches);
