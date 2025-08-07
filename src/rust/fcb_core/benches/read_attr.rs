use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use fcb_core::error::Result;
use fcb_core::static_btree::Float;
use fcb_core::{AttrQuery, FcbReader, KeyType, Operator};
use std::{fs::File, io::BufReader};

// TODO: test these cases as well
// document_nummers: ["314413.tif", "3179781", "1098447", "1119218", "1143007", "1098447", "1143007", "222593.tif", "1119218", "1268224", "4269547"]
// identifications: ["NL.IMBAG.Pand.0503100000012869", "NL.IMBAG.Pand.0503100000031124", "NL.IMBAG.Pand.0503100000016998", "NL.IMBAG.Pand.0503100000017005", "NL.IMBAG.Pand.0503100000019679", "NL.IMBAG.Pand.0503100000019699", "NL.IMBAG.Pand.0503100000017438", "NL.IMBAG.Pand.0503100000018308", "NL.IMBAG.Pand.0503100000022392", "NL.IMBAG.Pand.0503100000005249", "NL.IMBAG.Pand.0503100000027621"]

/// Read FCB file and count geometry types without attribute index.
fn read_fcb_without_attr_index(path: &str) -> Result<()> {
    let input_file = File::open(path)?;
    let input_reader = BufReader::new(input_file);

    let mut reader = FcbReader::open(input_reader)?.select_all()?;
    let header = reader.header();
    let feat_count = header.features_count();

    let mut target_feat_num = 0;
    let mut feat_total = 0;
    while let Some(feat_buf) = reader.next()? {
        let feature = feat_buf.cur_cj_feature()?;
        for (_, co) in feature.city_objects.iter() {
            if let Some(attributes) = &co.attributes {
                if let Some(identificatie) = attributes.get("identificatie") {
                    if identificatie.as_str().unwrap() == "NL.IMBAG.Pand.0503100000012869" {
                        target_feat_num += 1;
                        break;
                    }
                }
                // if let Some(b3_h_dak_50p) = attributes.get("b3_h_dak_50p") {
                //     if b3_h_dak_50p.as_f64().unwrap() > 2.0
                //     && b3_h_dak_50p.as_f64().unwrap() < 50.0
                //     {
                //         println!("b3_h_dak_50p: {:?}", b3_h_dak_50p);
                //         target_feat_num += 1;
                //         continue;
                //     }
                // }
            }
        }
        feat_total += 1;
        if feat_total == feat_count {
            break;
        }
    }
    Ok(())
}

/// Read FCB file and count geometry types using attribute index with seekable reader (StreamableMultiIndex).
fn read_fcb_with_attr_index_seekable(path: &str) -> Result<()> {
    let input_file = File::open(path)?;
    let input_reader = BufReader::new(input_file);

    let query: AttrQuery = vec![
        (
            "b3_h_dak_50p".to_string(),
            Operator::Gt,
            KeyType::Float64(Float(2.0)),
        ),
        (
            "b3_h_dak_50p".to_string(),
            Operator::Lt,
            KeyType::Float64(Float(50.0)),
        ),
        // (
        //     "identificatie".to_string(),
        //     Operator::Eq,
        //     ByteSerializableValue::String("NL.IMBAG.Pand.0503100000012869".to_string()),
        // ),
    ];

    // Use the seekable version with StreamableMultiIndex
    let mut reader = FcbReader::open(input_reader)?.select_attr_query(query)?;
    let header = reader.header();
    let feat_count = header.features_count();

    let mut target_feat_num = 0;
    let mut feat_total = 0;
    while let Some(feat_buf) = reader.next()? {
        let feature = feat_buf.cur_cj_feature()?;
        for (_, co) in feature.city_objects.iter() {
            if let Some(attributes) = &co.attributes {
                if let Some(b3_h_dak_50p) = attributes.get("b3_h_dak_50p") {
                    if b3_h_dak_50p.as_f64().unwrap() > 2.0 && b3_h_dak_50p.as_f64().unwrap() < 50.0
                    {
                        println!("b3_h_dak_50p: {b3_h_dak_50p:?}");
                        target_feat_num += 1;
                        continue;
                    }
                }
                // if let Some(identificatie) = attributes.get("identificatie") {
                //     if identificatie.as_str().unwrap() == "NL.IMBAG.Pand.0503100000012869" {
                //         target_feat_num += 1;
                //         break;
                //     }
                // }
            }
        }
        feat_total += 1;
        if feat_total == feat_count {
            break;
        }
    }

    Ok(())
}

/// Read FCB file and count geometry types using attribute index with non-seekable reader (optimized MultiIndex).
fn read_fcb_with_attr_index_non_seekable(path: &str) -> Result<()> {
    let input_file = File::open(path)?;
    let input_reader = BufReader::new(input_file);

    let query: AttrQuery = vec![
        (
            "b3_h_dak_50p".to_string(),
            Operator::Gt,
            KeyType::Float64(Float(2.0)),
        ),
        (
            "b3_h_dak_50p".to_string(),
            Operator::Lt,
            KeyType::Float64(Float(50.0)),
        ),
        // (
        //     "identificatie".to_string(),
        //     Operator::Eq,
        //     KeyType::StringKey50(FixedStringKey::from_str("NL.IMBAG.Pand.0503100000012869")),
        // ),
    ];

    // Use the non-seekable version with optimized MultiIndex
    let mut reader = FcbReader::open(input_reader)?.select_attr_query_seq(query)?;
    let header = reader.header();
    let feat_count = header.features_count();

    let mut target_feat_num = 0;
    let mut feat_total = 0;
    while let Some(feat_buf) = reader.next()? {
        let feature = feat_buf.cur_cj_feature()?;
        for (_, co) in feature.city_objects.iter() {
            if let Some(attributes) = &co.attributes {
                if let Some(b3_h_dak_50p) = attributes.get("b3_h_dak_50p") {
                    if b3_h_dak_50p.as_f64().unwrap() > 2.0 && b3_h_dak_50p.as_f64().unwrap() < 50.0
                    {
                        println!("b3_h_dak_50p: {b3_h_dak_50p:?}");
                        target_feat_num += 1;
                        continue;
                    }
                }
                // if let Some(identificatie) = attributes.get("identificatie") {
                //     if identificatie.as_str().unwrap() == "NL.IMBAG.Pand.0503100000012869" {
                //         target_feat_num += 1;
                //         break;
                //     }
                // }
            }
        }
        feat_total += 1;
        if feat_total == feat_count {
            break;
        }
    }

    Ok(())
}

const DATASETS: &[(&str, (&str, &str))] = &[(
    "delft",
    (
        // "benchmark_data/attribute/delft.fcb",
        "benchmark_data/attribute/3dbag_partial.fcb",
        "benchmark_data/attribute/3dbag_partial.fcb",
    ),
)];

pub fn read_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("read");

    // Set sample size to 10 to limit the number of iterations
    group.sample_size(10);

    for &(dataset, (file_without, file_with)) in DATASETS.iter() {
        // Benchmark the file without attribute index.
        // group.bench_with_input(
        //     BenchmarkId::new(format!("{} without", dataset), file_without),
        //     &file_without,
        //     |b, &path| {
        //         b.iter(|| {
        //             read_fcb_without_attr_index(path).unwrap();
        //         })
        //     },
        // );

        // Benchmark the file with attribute index using seekable reader.
        group.bench_with_input(
            BenchmarkId::new(format!("{dataset} with seekable (streamable)"), file_with),
            &file_with,
            |b, &path| {
                b.iter(|| {
                    read_fcb_with_attr_index_seekable(path).unwrap();
                })
            },
        );

        // Benchmark the file with attribute index using non-seekable reader.
        group.bench_with_input(
            BenchmarkId::new(
                format!("{dataset} with non-seekable (sequential)"),
                file_with,
            ),
            &file_with,
            |b, &path| {
                b.iter(|| {
                    read_fcb_with_attr_index_non_seekable(path).unwrap();
                })
            },
        );
    }

    group.finish();

    // Optionally print a concise summary.
    println!("\nBenchmark Results:");
    println!("{:<12} {:<25} {:<15}", "Dataset", "Method", "Mean Time");
    println!("{:-<52}", "");
}

criterion_group!(benches, read_benchmark);
criterion_main!(benches);
