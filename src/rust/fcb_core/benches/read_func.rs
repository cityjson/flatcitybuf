use anyhow::Result;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use fcb_core::{FcbReader, GeometryType};
use prettytable::{format, Cell, Row, Table};
use std::{
    collections::HashMap,
    fs::File,
    io::BufReader,
    time::{Duration, Instant},
};

/// Read FCB file using cur_feature method for accessing feature data
fn read_fcb_with_cur_feature(path: &str) -> Result<(u64, u64, u64)> {
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

/// Read FCB file using cur_cj_feature method to access feature data in CityJSON representation
fn read_fcb_with_cur_cj_feature(path: &str) -> Result<(u64, u64, u64)> {
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

#[derive(Debug)]
struct BenchResult {
    function: String,
    duration: Duration,
}

fn format_duration(d: Duration) -> String {
    if d.as_secs() > 0 {
        format!("{:.2}s", d.as_secs_f64())
    } else {
        format!("{:.2}ms", d.as_secs_f64() * 1000.0)
    }
}

/// Benchmark a read function
fn benchmark_read_fn<F>(
    iterations: u32,
    function_name: &str,
    path: &str,
    read_fn: F,
) -> Result<BenchResult>
where
    F: Fn(&str) -> Result<(u64, u64, u64)>,
{
    let mut total_duration = Duration::new(0, 0);

    for i in 0..iterations {
        // Execute the read function and measure time
        let iter_start = Instant::now();
        let _ = read_fn(black_box(path))?;
        let iter_duration = iter_start.elapsed();
        total_duration += iter_duration;

        // Optional progress reporting
        if iterations > 1 && (i + 1) % (iterations / 10).max(1) == 0 {
            println!(
                "progress: {}/{} iterations for {} - {}",
                i + 1,
                iterations,
                function_name,
                path
            );
        }
    }

    // Calculate average
    let avg_duration = if iterations > 0 {
        total_duration / iterations
    } else {
        Duration::new(0, 0)
    };

    Ok(BenchResult {
        function: function_name.to_string(),
        duration: avg_duration,
    })
}

/// Print benchmark results using prettytable
fn print_benchmark_results(results: &HashMap<String, HashMap<String, BenchResult>>) {
    // Create a formatted table for results
    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);

    // Create header row with dataset names
    let mut header_cells = vec![Cell::new("Function")];
    let datasets: Vec<&String> = results.keys().collect();
    datasets.iter().for_each(|dataset| {
        header_cells.push(Cell::new(dataset));
    });
    table.add_row(Row::new(header_cells));

    // Get all unique function names
    let mut all_functions = Vec::new();
    for dataset_results in results.values() {
        for function_name in dataset_results.keys() {
            if !all_functions.contains(function_name) {
                all_functions.push(function_name.clone());
            }
        }
    }
    all_functions.sort();

    // Add a row for each function
    for function_name in &all_functions {
        let mut row_cells = vec![Cell::new(function_name)];

        for dataset in &datasets {
            if let Some(result) = results.get(*dataset).and_then(|dr| dr.get(function_name)) {
                row_cells.push(Cell::new(&format_duration(result.duration)));
            } else {
                row_cells.push(Cell::new("N/A"));
            }
        }

        table.add_row(Row::new(row_cells));
    }

    // Print the table
    println!("\nFunction Benchmark Results:");
    table.printstd();

    // Create a comparison table showing relative performance
    let mut comparison_table = Table::new();
    comparison_table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);

    // Add header row
    let mut comp_header_cells = vec![Cell::new("Dataset")];
    for function_name in &all_functions {
        comp_header_cells.push(Cell::new(function_name));
    }
    comparison_table.add_row(Row::new(comp_header_cells));

    // Add a row for each dataset showing relative performance
    for dataset in &datasets {
        let mut row_cells = vec![Cell::new(dataset)];

        // Find fastest function for this dataset to use as baseline
        let mut baseline_duration = Duration::from_secs(u64::MAX);
        if let Some(dataset_results) = results.get(*dataset) {
            for result in dataset_results.values() {
                if result.duration < baseline_duration {
                    baseline_duration = result.duration;
                }
            }
        }

        // Calculate relative performance for each function
        for function_name in &all_functions {
            if let Some(result) = results.get(*dataset).and_then(|dr| dr.get(function_name)) {
                let relative = result.duration.as_secs_f64() / baseline_duration.as_secs_f64();
                row_cells.push(Cell::new(&format!("{relative:.2}x")));
            } else {
                row_cells.push(Cell::new("N/A"));
            }
        }

        comparison_table.add_row(Row::new(row_cells));
    }

    // Print the comparison table
    println!("\nRelative Performance (lower is better):");
    comparison_table.printstd();
}

// Subset of datasets for function benchmarking
const DATASETS: &[(&str, &str)] = &[
    // ("3DBAG", "benchmark_data/3DBAG.city.fcb"),
    // ("Helsinki", "benchmark_data/Helsinki.city.fcb"),
    // ("Rotterdam", "benchmark_data/Rotterdam.fcb"),
    // ("Zurich", "benchmark_data/Zurich.city.fcb"),
    ("Ingolstadt", "benchmark_data/Ingolstadt.city.fcb"),
];

pub fn read_func_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("read_func");

    let iterations: u32 = 10;
    // Increase warm-up time and measurement time to prevent timeouts
    group
        .sample_size(iterations as usize)
        .warm_up_time(Duration::from_secs(5));

    let mut all_results: HashMap<String, HashMap<String, BenchResult>> = HashMap::new();

    // Create a real-time results table
    let mut real_time_table = Table::new();
    real_time_table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);

    // Add header row
    real_time_table.add_row(Row::new(vec![
        Cell::new("Dataset"),
        Cell::new("Function"),
        Cell::new("Duration"),
    ]));

    println!("\nFunction Benchmark Results (Real-time):");
    real_time_table.printstd();

    for (dataset_name, fcb_path) in DATASETS {
        let mut dataset_results = HashMap::new();

        // Create a table for this dataset's results
        let mut dataset_table = Table::new();
        dataset_table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
        dataset_table.add_row(Row::new(vec![Cell::new("Function"), Cell::new("Duration")]));

        // Benchmark cur_feature
        println!("benchmarking cur_feature for dataset: {dataset_name}");
        let result = benchmark_read_fn(
            iterations,
            "cur_feature",
            fcb_path,
            read_fcb_with_cur_feature,
        )
        .unwrap_or_else(|e| {
            println!("error in cur_feature benchmark: {e:?}");
            BenchResult {
                function: "cur_feature".to_string(),
                duration: Duration::new(0, 0),
            }
        });

        // Add to dataset table
        dataset_table.add_row(Row::new(vec![
            Cell::new(&result.function),
            Cell::new(&format_duration(result.duration)),
        ]));

        group.bench_with_input(
            BenchmarkId::new("cur_feature", dataset_name),
            fcb_path,
            |b, path| b.iter(|| read_fcb_with_cur_feature(black_box(path))),
        );

        dataset_results.insert("cur_feature".to_string(), result);

        // Benchmark cur_cj_feature
        println!("benchmarking cur_cj_feature for dataset: {dataset_name}");
        let result = benchmark_read_fn(
            iterations,
            "cur_cj_feature",
            fcb_path,
            read_fcb_with_cur_cj_feature,
        )
        .unwrap_or_else(|e| {
            println!("error in cur_cj_feature benchmark: {e:?}");
            BenchResult {
                function: "cur_cj_feature".to_string(),
                duration: Duration::new(0, 0),
            }
        });

        // Add to dataset table
        dataset_table.add_row(Row::new(vec![
            Cell::new(&result.function),
            Cell::new(&format_duration(result.duration)),
        ]));

        group.bench_with_input(
            BenchmarkId::new("cur_cj_feature", dataset_name),
            fcb_path,
            |b, path| b.iter(|| read_fcb_with_cur_cj_feature(black_box(path))),
        );

        dataset_results.insert("cur_cj_feature".to_string(), result);

        // Print the dataset results
        println!("\nResults for dataset: {dataset_name}");
        dataset_table.printstd();

        // Store all results for this dataset
        all_results.insert(dataset_name.to_string(), dataset_results);
    }

    group.finish();

    // Print comprehensive results and comparison
    print_benchmark_results(&all_results);
}

criterion_group!(benches, read_func_benchmark);
criterion_main!(benches);
