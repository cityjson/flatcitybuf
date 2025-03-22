use anyhow::Result;
use bson::Document;
use cjseq::{CityJSON, CityJSONFeature};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use fcb_core::{FcbReader, GeometryType};
use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
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

    println!("solid_count: {}", solid_count);
    println!("multi_surface_count: {}", multi_surface_count);
    println!("other_count: {}", other_count);
    println!("feat_count: {}", feat_count);

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

    println!("solid_count: {}", solid_count);
    println!("multi_surface_count: {}", multi_surface_count);
    println!("other_count: {}", other_count);
    println!("feat_count: {}", feat_count);

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

    println!("solid_count: {}", solid_count);
    println!("multi_surface_count: {}", multi_surface_count);
    println!("other_count: {}", other_count);

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

    println!("solid_count: {}", solid_count);
    println!("multi_surface_count: {}", multi_surface_count);
    println!("other_count: {}", other_count);

    Ok((solid_count, multi_surface_count, other_count))
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_read_counts_match() -> Result<()> {
        let fcb_path = "benchmark_data/3DBAG.fcb";
        let cjseq_path = "benchmark_data/3DBAG.city.jsonl";

        let (fcb_solids, fcb_surfaces, fcb_others) = read_fcb(fcb_path)?;
        let (cj_solids, cj_surfaces, cj_others) = read_cjseq(cjseq_path)?;

        assert_eq!(fcb_solids, cj_solids, "solid counts don't match");
        assert_eq!(fcb_surfaces, cj_surfaces, "surface counts don't match");
        assert_eq!(fcb_others, cj_others, "other geometry counts don't match");

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
    // (
    //     "Helsinki",
    //     (
    //         "benchmark_data/Helsinki.city.fcb",
    //         "benchmark_data/Helsinki.city.jsonl",
    //         "benchmark_data/Helsinki.city.cbor",
    //         "benchmark_data/Helsinki.city.bson",
    //     ),
    // ),
    // (
    //     "Ingolstadt",
    //     (
    //         "benchmark_data/Ingolstadt.city.fcb",
    //         "benchmark_data/Ingolstadt.city.jsonl",
    //         "benchmark_data/Ingolstadt.city.cbor",
    //         "benchmark_data/Ingolstadt.city.bson",
    //     ),
    // ),
    // (
    //     "Montreal",
    //     (
    //         "benchmark_data/Montreal.city.fcb",
    //         "benchmark_data/Montreal.city.jsonl",
    //         "benchmark_data/Montreal.city.cbor",
    //         "benchmark_data/Montreal.city.bson",
    //     ),
    // ),
    // (
    //     "NYC",
    //     (
    //         "benchmark_data/NYC.fcb",
    //         "benchmark_data/NYC.jsonl",
    //         "benchmark_data/NYC.cbor",
    //         "benchmark_data/NYC.bson",
    //     ),
    // ),
    // (
    //     "Rotterdam",
    //     (
    //         "benchmark_data/Rotterdam.fcb",
    //         "benchmark_data/Rotterdam.jsonl",
    //         "benchmark_data/Rotterdam.cbor",
    //         "benchmark_data/Rotterdam.bson",
    //     ),
    // ),
    // (
    //     "Vienna",
    //     (
    //         "benchmark_data/Vienna.city.fcb",
    //         "benchmark_data/Vienna.city.jsonl",
    //         "benchmark_data/Vienna.city.cbor",
    //         "benchmark_data/Vienna.city.bson",
    //     ),
    // ),
    // (
    //     "Zurich",
    //     (
    //         "benchmark_data/Zurich.city.fcb",
    //         "benchmark_data/Zurich.city.jsonl",
    //         "benchmark_data/Zurich.city.cbor",
    //         "benchmark_data/Zurich.city.bson",
    //     ),
    // ),
];

fn format_duration(d: Duration) -> String {
    if d.as_secs() > 0 {
        format!("{:.2}s", d.as_secs_f64())
    } else {
        format!("{:.2}ms", d.as_millis() as f64)
    }
}

#[derive(Debug)]
struct BenchResult {
    format: String,
    duration: Duration,
}

pub fn read_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("read");

    let iterations: u32 = 10;
    // Only specify sample size and minimal warm-up
    group
        .sample_size(iterations as usize)
        .warm_up_time(Duration::from_millis(500));

    let mut results = HashMap::new();

    for (size, (fcb_path, cjseq_path, cbor_path, bson_path)) in DATASETS {
        // FCB benchmark
        let start = Instant::now();
        group.bench_with_input(BenchmarkId::new("fcb", size), fcb_path, |b, path| {
            b.iter(|| read_fcb(black_box(path)))
        });
        results.insert(
            format!("{}_fcb", size),
            BenchResult {
                format: "FCB".to_string(),
                duration: start.elapsed() / iterations,
            },
        );

        // CJSeq benchmark
        let start = Instant::now();
        group.bench_with_input(BenchmarkId::new("cjseq", size), cjseq_path, |b, path| {
            b.iter(|| read_cjseq(black_box(path)))
        });
        results.insert(
            format!("{}_cjseq", size),
            BenchResult {
                format: "CJSeq".to_string(),
                duration: start.elapsed() / iterations,
            },
        );

        // CBOR benchmark
        let start = Instant::now();
        group.bench_with_input(BenchmarkId::new("cbor", size), cbor_path, |b, path| {
            b.iter(|| read_cbor(black_box(path)))
        });
        results.insert(
            format!("{}_cbor", size),
            BenchResult {
                format: "CBOR".to_string(),
                duration: start.elapsed() / iterations,
            },
        );

        // BSON benchmark
        let start = Instant::now();
        group.bench_with_input(BenchmarkId::new("bson", size), bson_path, |b, path| {
            b.iter(|| read_bson(black_box(path)))
        });
        results.insert(
            format!("{}_bson", size),
            BenchResult {
                format: "BSON".to_string(),
                duration: start.elapsed() / iterations,
            },
        );
    }

    group.finish();

    // Print all results at the end
    println!("\nBenchmark Results:");
    println!("{:<12} {:<15} {:<15}", "Dataset", "Format", "Mean Time");
    println!("{:-<42}", "");

    for (size, _) in DATASETS {
        for format in &["fcb", "cjseq", "cbor", "bson"] {
            if let Some(result) = results.get(&format!("{}_{}", size, format)) {
                println!(
                    "{:<12} {:<15} {}",
                    size,
                    result.format,
                    format_duration(result.duration)
                );
            }
        }
    }
}

criterion_group!(benches, read_benchmark);
criterion_main!(benches);
