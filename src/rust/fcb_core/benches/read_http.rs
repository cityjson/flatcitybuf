use anyhow::{Context, Result};
use fcb_core::packed_rtree::Query;
use fcb_core::{FixedStringKey, HttpFcbReader, KeyType, Operator};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::time::{Duration, Instant};

// Configuration for the benchmark
const ITERATIONS: u32 = 50;
const WARMUP_ITERATIONS: u32 = 10;
const FCB_URL: &str = "https://storage.googleapis.com/flatcitybuf/3dbag_all_index.fcb";
const THREEBAG_API_URL: &str = "https://api.3dbag.nl/collections/pand/items";

// Test feature IDs to benchmark
const TEST_FEATURE_IDS: &[&str] = &[
    "NL.IMBAG.Pand.0503100000032914", //TUDelft BK building
    "NL.IMBAG.Pand.0363100012185598", //Amsterdam central station
    "NL.IMBAG.Pand.0014100010938997", //Groningen station
    "NL.IMBAG.Pand.0772100000295227", //Eindhoven station
    "NL.IMBAG.Pand.0153100000261851", //Enschede station
];

// Bounding box for bbox benchmark (minx, miny, maxx, maxy) 1km x 1km
const BBOX_COORDS: (f64, f64, f64, f64) = (84000.0, 444000.0, 86000.0, 446000.0);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BenchmarkResult {
    method: String,
    feature_id: String,
    iterations: u32,
    mean_duration_ms: f64,
    median_duration_ms: f64,
    std_dev_duration_ms: f64,
    min_duration_ms: f64,
    max_duration_ms: f64,
    success_rate: f64,
    total_bytes_transferred: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct PandResponse {
    feature: Value,
    id: String,
    links: Vec<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BboxResponse {
    #[serde(rename = "type")]
    response_type: String,
    features: Vec<Value>,
    #[serde(rename = "numberMatched")]
    number_matched: Option<u64>,
    #[serde(rename = "numberReturned")]
    number_returned: Option<u64>,
    links: Vec<Value>,
    #[serde(rename = "timeStamp")]
    time_stamp: Option<String>,
}

/// Benchmark FlatCityBuf HTTP reading for a specific feature
async fn http_read_fcb_pand(feature_id: &str) -> Result<(Duration, u64)> {
    let start = Instant::now();

    let http_reader = HttpFcbReader::open(FCB_URL)
        .await
        .context("failed to open FCB HTTP reader")?;

    let query: Vec<(String, Operator, KeyType)> = vec![(
        "identificatie".to_string(),
        Operator::Eq,
        KeyType::StringKey50(FixedStringKey::from_str(feature_id)),
    )];

    let mut iter = http_reader
        .select_attr_query(&query)
        .await
        .context("failed to execute attribute query")?;

    let mut features_found = 0;
    let mut total_bytes = 0u64;

    while let Some(feature) = iter.next().await? {
        let bytes = feature.features_buf.len();
        total_bytes += bytes as u64;
        features_found += 1;
    }

    let duration = start.elapsed();

    if features_found == 0 {
        return Err(anyhow::anyhow!("no features found for ID: {}", feature_id));
    }

    Ok((duration, total_bytes))
}

/// Benchmark FlatCityBuf HTTP reading for bounding box query
async fn http_read_fcb_bbox() -> Result<(Duration, u64)> {
    let start = Instant::now();

    let http_reader = HttpFcbReader::open(FCB_URL)
        .await
        .context("failed to open FCB HTTP reader")?;

    let (minx, miny, maxx, maxy) = BBOX_COORDS;
    let mut iter = http_reader
        .select_query(Query::BBox(minx, miny, maxx, maxy))
        .await
        .context("failed to execute bbox query")?;

    let mut features_found = 0;
    let mut total_bytes = 0u64;

    // Limit to max 10 features as requested
    loop {
        if features_found >= 10 {
            break;
        }

        match iter.next().await? {
            Some(feature) => {
                let bytes = feature.features_buf.len();
                total_bytes += bytes as u64;
                features_found += 1;
            }
            None => break,
        }
    }

    let duration = start.elapsed();

    if features_found == 0 {
        return Err(anyhow::anyhow!("no features found in bbox"));
    }

    println!("  found {features_found} features in bbox");
    Ok((duration, total_bytes))
}

/// Benchmark 3DBAG API reading for a specific feature
async fn http_read_3dbag_pand(
    client: &reqwest::Client,
    feature_id: &str,
) -> Result<(Duration, u64)> {
    let start = Instant::now();

    let url = format!("{THREEBAG_API_URL}/{feature_id}");

    let response = client
        .get(&url)
        .header("Accept", "application/city+json")
        .send()
        .await
        .context("failed to send request to 3DBAG API")?;

    let content_length = response.content_length().unwrap_or(0);

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "3DBAG API returned status: {}",
            response.status()
        ));
    }

    let response_text = response
        .text()
        .await
        .context("failed to read response body")?;

    let response_data: PandResponse =
        serde_json::from_str(&response_text).context("failed to parse 3DBAG API response")?;

    let duration = start.elapsed();

    if response_data.feature.is_null() {
        return Err(anyhow::anyhow!("no features found for ID: {}", feature_id));
    }

    // Use actual response size if content-length wasn't available
    let bytes_transferred = if content_length > 0 {
        content_length
    } else {
        response_text.len() as u64
    };

    Ok((duration, bytes_transferred))
}

/// Benchmark 3DBAG API reading for bounding box query
async fn http_read_3dbag_bbox(client: &reqwest::Client) -> Result<(Duration, u64)> {
    let start = Instant::now();

    let (minx, miny, maxx, maxy) = BBOX_COORDS;
    let url = format!("{THREEBAG_API_URL}?bbox={minx},{miny},{maxx},{maxy}&limit=10");

    let response = client
        .get(&url)
        .header("Accept", "application/city+json")
        .send()
        .await
        .context("failed to send bbox request to 3DBAG API")?;

    let content_length = response.content_length().unwrap_or(0);

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "3DBAG API returned status: {}",
            response.status()
        ));
    }

    let response_text = response
        .text()
        .await
        .context("failed to read response body")?;

    let response_data: BboxResponse =
        serde_json::from_str(&response_text).context("failed to parse 3DBAG bbox API response")?;

    let duration = start.elapsed();

    if response_data.number_returned.is_none() {
        return Err(anyhow::anyhow!("no features found in bbox"));
    }
    println!("  number returned: {:?}", response_data.number_returned);

    // Use actual response size if content-length wasn't available
    let bytes_transferred = if content_length > 0 {
        content_length
    } else {
        response_text.len() as u64
    };

    Ok((duration, bytes_transferred))
}

/// Calculate statistical metrics from a vector of durations
fn calculate_statistics(durations: &[Duration]) -> (f64, f64, f64, f64, f64) {
    if durations.is_empty() {
        return (0.0, 0.0, 0.0, 0.0, 0.0);
    }

    let mut sorted_durations = durations.to_vec();
    sorted_durations.sort();

    let mean_ms = durations
        .iter()
        .map(|d| d.as_secs_f64() * 1000.0)
        .sum::<f64>()
        / durations.len() as f64;

    let median_ms = if sorted_durations.len() % 2 == 0 {
        let mid1 = sorted_durations[sorted_durations.len() / 2 - 1].as_secs_f64() * 1000.0;
        let mid2 = sorted_durations[sorted_durations.len() / 2].as_secs_f64() * 1000.0;
        (mid1 + mid2) / 2.0
    } else {
        sorted_durations[sorted_durations.len() / 2].as_secs_f64() * 1000.0
    };

    let variance = durations
        .iter()
        .map(|d| {
            let diff = d.as_secs_f64() * 1000.0 - mean_ms;
            diff * diff
        })
        .sum::<f64>()
        / durations.len() as f64;

    let std_dev_ms = variance.sqrt();
    let min_ms = sorted_durations[0].as_secs_f64() * 1000.0;
    let max_ms = sorted_durations[sorted_durations.len() - 1].as_secs_f64() * 1000.0;

    (mean_ms, median_ms, std_dev_ms, min_ms, max_ms)
}

/// Run benchmark for a specific method and feature ID
async fn run_benchmark(
    method: &str,
    feature_id: &str,
    client: &reqwest::Client,
) -> Result<BenchmarkResult> {
    println!("benchmarking {method} for feature: {feature_id}");

    // Warm-up iterations
    println!("  performing {WARMUP_ITERATIONS} warm-up iterations...");
    for i in 0..WARMUP_ITERATIONS {
        let result = match method {
            "FlatCityBuf" => http_read_fcb_pand(feature_id).await,
            "3DBAG_API" => http_read_3dbag_pand(client, feature_id).await,
            "FlatCityBuf_BBox" => http_read_fcb_bbox().await,
            "3DBAG_API_BBox" => http_read_3dbag_bbox(client).await,
            _ => return Err(anyhow::anyhow!("unknown method: {}", method)),
        };

        if result.is_err() && i == 0 {
            println!("    warning: warm-up iteration failed: {:?}", result.err());
        }
    }

    // Measured iterations
    println!("  starting {ITERATIONS} measured iterations...");
    let mut durations = Vec::with_capacity(ITERATIONS as usize);
    let mut total_bytes = 0u64;
    let mut successful_iterations = 0u32;

    for i in 0..ITERATIONS {
        if i % 20 == 0 && i > 0 {
            println!("    completed {i}/{ITERATIONS} iterations");
        }

        let result = match method {
            "FlatCityBuf" => http_read_fcb_pand(feature_id).await,
            "3DBAG_API" => http_read_3dbag_pand(client, feature_id).await,
            "FlatCityBuf_BBox" => http_read_fcb_bbox().await,
            "3DBAG_API_BBox" => http_read_3dbag_bbox(client).await,
            _ => return Err(anyhow::anyhow!("unknown method: {}", method)),
        };

        match result {
            Ok((duration, bytes)) => {
                durations.push(duration);
                total_bytes += bytes;
                successful_iterations += 1;
            }
            Err(e) => {
                eprintln!("    iteration {} failed: {:?}", i + 1, e);
            }
        }
    }

    if successful_iterations == 0 {
        return Err(anyhow::anyhow!(
            "all iterations failed for {} with feature {}",
            method,
            feature_id
        ));
    }

    let (mean_ms, median_ms, std_dev_ms, min_ms, max_ms) = calculate_statistics(&durations);
    let success_rate = successful_iterations as f64 / ITERATIONS as f64 * 100.0;

    println!(
        "  completed: mean={mean_ms:.2}ms, median={median_ms:.2}ms, std_dev={std_dev_ms:.2}ms, success_rate={success_rate:.1}%"
    );

    Ok(BenchmarkResult {
        method: method.to_string(),
        feature_id: feature_id.to_string(),
        iterations: successful_iterations,
        mean_duration_ms: mean_ms,
        median_duration_ms: median_ms,
        std_dev_duration_ms: std_dev_ms,
        min_duration_ms: min_ms,
        max_duration_ms: max_ms,
        success_rate,
        total_bytes_transferred: total_bytes,
    })
}

/// Print comprehensive benchmark results
fn print_results(results: &[BenchmarkResult]) {
    println!("\n{:=<120}", "");
    println!("HTTP BENCHMARK RESULTS - FlatCityBuf vs 3DBAG API");
    println!("{:=<120}", "");

    // Group results by feature ID
    let mut results_by_feature: HashMap<String, Vec<&BenchmarkResult>> = HashMap::new();
    for result in results {
        results_by_feature
            .entry(result.feature_id.clone())
            .or_default()
            .push(result);
    }

    // Print detailed results for each feature
    for (feature_id, feature_results) in &results_by_feature {
        println!("\nFeature ID: {feature_id}");
        println!("{:-<120}", "");
        println!(
            "{:<15} {:>10} {:>12} {:>12} {:>12} {:>12} {:>12} {:>12} {:>15}",
            "Method",
            "Success%",
            "Mean (ms)",
            "Median (ms)",
            "Std Dev",
            "Min (ms)",
            "Max (ms)",
            "Iterations",
            "Bytes"
        );
        println!("{:-<120}", "");

        for result in feature_results {
            println!(
                "{:<15} {:>9.1}% {:>11.2} {:>11.2} {:>11.2} {:>11.2} {:>11.2} {:>11} {:>14}",
                result.method,
                result.success_rate,
                result.mean_duration_ms,
                result.median_duration_ms,
                result.std_dev_duration_ms,
                result.min_duration_ms,
                result.max_duration_ms,
                result.iterations,
                format_bytes(result.total_bytes_transferred)
            );
        }

        // Calculate comparison if both methods are present
        if feature_results.len() == 2 {
            let fcb_result = feature_results.iter().find(|r| r.method == "FlatCityBuf");
            let api_result = feature_results.iter().find(|r| r.method == "3DBAG_API");

            if let (Some(fcb), Some(api)) = (fcb_result, api_result) {
                let speed_ratio = api.mean_duration_ms / fcb.mean_duration_ms;
                let bytes_ratio =
                    api.total_bytes_transferred as f64 / fcb.total_bytes_transferred as f64;

                println!("{:-<120}", "");
                println!(
                    "Comparison: FlatCityBuf is {:.2}x faster, transfers {:.2}x {} data",
                    speed_ratio,
                    bytes_ratio,
                    if bytes_ratio > 1.0 { "less" } else { "more" }
                );
            }
        }
    }

    // Overall summary
    println!("\n{:=<120}", "");
    println!("OVERALL SUMMARY");
    println!("{:=<120}", "");

    let fcb_results: Vec<_> = results
        .iter()
        .filter(|r| r.method == "FlatCityBuf")
        .collect();
    let api_results: Vec<_> = results.iter().filter(|r| r.method == "3DBAG_API").collect();

    if !fcb_results.is_empty() && !api_results.is_empty() {
        let fcb_avg_time =
            fcb_results.iter().map(|r| r.mean_duration_ms).sum::<f64>() / fcb_results.len() as f64;
        let api_avg_time =
            api_results.iter().map(|r| r.mean_duration_ms).sum::<f64>() / api_results.len() as f64;

        let fcb_avg_bytes = fcb_results
            .iter()
            .map(|r| r.total_bytes_transferred)
            .sum::<u64>()
            / fcb_results.len() as u64;
        let api_avg_bytes = api_results
            .iter()
            .map(|r| r.total_bytes_transferred)
            .sum::<u64>()
            / api_results.len() as u64;

        println!("Average Performance:");
        println!(
            "  FlatCityBuf: {:.2}ms, {} transferred",
            fcb_avg_time,
            format_bytes(fcb_avg_bytes)
        );
        println!(
            "  3DBAG API:   {:.2}ms, {} transferred",
            api_avg_time,
            format_bytes(api_avg_bytes)
        );
        println!(
            "  Speed Ratio: {:.2}x (FlatCityBuf is {})",
            api_avg_time / fcb_avg_time,
            if api_avg_time > fcb_avg_time {
                "faster"
            } else {
                "slower"
            }
        );
    }
}

/// Format bytes in human-readable format
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

/// Export results to CSV
fn export_results_to_csv(results: &[BenchmarkResult]) -> Result<()> {
    use std::fs::File;
    use std::io::Write;

    let filename = "http_benchmark_results.csv";
    let mut file = File::create(filename).context("failed to create CSV file")?;

    writeln!(file, "Method,FeatureID,Iterations,MeanDurationMs,MedianDurationMs,StdDevDurationMs,MinDurationMs,MaxDurationMs,SuccessRate,TotalBytesTransferred")?;

    for result in results {
        writeln!(
            file,
            "{},{},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.2},{}",
            result.method,
            result.feature_id,
            result.iterations,
            result.mean_duration_ms,
            result.median_duration_ms,
            result.std_dev_duration_ms,
            result.min_duration_ms,
            result.max_duration_ms,
            result.success_rate,
            result.total_bytes_transferred
        )?;
    }

    println!("results exported to: {filename}");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("starting HTTP benchmark: FlatCityBuf vs 3DBAG API");
    println!("iterations: {ITERATIONS}, warm-up: {WARMUP_ITERATIONS}");
    println!("test features: {TEST_FEATURE_IDS:?}");
    println!("bbox coordinates: {BBOX_COORDS:?}");
    println!();

    // Create HTTP client for 3DBAG API
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("failed to create HTTP client")?;

    let mut all_results = Vec::new();

    // Run benchmarks for each feature ID
    for feature_id in TEST_FEATURE_IDS {
        println!("testing feature: {feature_id}");

        // Benchmark FlatCityBuf
        match run_benchmark("FlatCityBuf", feature_id, &client).await {
            Ok(result) => all_results.push(result),
            Err(e) => eprintln!("FlatCityBuf benchmark failed for {feature_id}: {e:?}"),
        }

        // Benchmark 3DBAG API
        match run_benchmark("3DBAG_API", feature_id, &client).await {
            Ok(result) => all_results.push(result),
            Err(e) => eprintln!("3DBAG API benchmark failed for {feature_id}: {e:?}"),
        }

        println!();
    }

    // Run bbox benchmarks
    println!("testing bbox query");

    // Benchmark FlatCityBuf BBox
    match run_benchmark("FlatCityBuf_BBox", "bbox_query", &client).await {
        Ok(result) => all_results.push(result),
        Err(e) => eprintln!("FlatCityBuf bbox benchmark failed: {e:?}"),
    }

    // Benchmark 3DBAG API BBox
    match run_benchmark("3DBAG_API_BBox", "bbox_query", &client).await {
        Ok(result) => all_results.push(result),
        Err(e) => eprintln!("3DBAG API bbox benchmark failed: {e:?}"),
    }

    // Print and export results
    print_results(&all_results);
    export_results_to_csv(&all_results)?;

    println!("\n{:=<120}", "");
    println!("HTTP BENCHMARK COMPLETED");
    println!("{:=<120}", "");

    Ok(())
}
