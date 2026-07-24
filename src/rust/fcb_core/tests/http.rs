use std::error::Error;

use anyhow::Result;
use fcb_core::packed_rtree::Query;
use fcb_core::Float;
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
use fcb_core::HttpFcbReader;
use fcb_core::{deserializer::to_cj_metadata, KeyType, Operator};

async fn read_http_file_bbox(path: &str) -> Result<(), Box<dyn Error>> {
    let http_reader = HttpFcbReader::open(path).await?;
    let minx = 68989.19384501831;
    let miny = 444614.3991728433;
    let maxx = 70685.16687543111;
    let maxy = 446023.6031208569;

    let mut iter = http_reader
        .select_query(Query::BBox(minx, miny, maxx, maxy))
        .await?;
    let header = iter.header();
    let _cj = to_cj_metadata(&header)?;

    // let mut writer = BufWriter::new(File::create("delft_http.city.jsonl")?);
    // writeln!(writer, "{}", serde_json::to_string(&cj)?)?;

    let mut feat_num = 0;
    let feat_count = header.features_count();
    let mut features = Vec::new();
    while let Some(feature) = iter.next().await? {
        let cj_feature = feature.cj_feature()?;
        features.push(cj_feature);
        // writeln!(writer, "{}", serde_json::to_string(&cj_feature)?)?;

        feat_num += 1;
        if feat_num >= feat_count {
            break;
        }
    }
    // TODO: add more tests
    Ok(())
}

async fn read_http_file_attr(path: &str) -> Result<(), Box<dyn Error>> {
    let http_reader = HttpFcbReader::open(path).await?;
    let query: Vec<(String, Operator, KeyType)> = vec![
        (
            "b3_h_dak_50p".to_string(),
            Operator::Gt,
            KeyType::Float64(Float(300.0)),
        ),
        // (
        //     "identificatie".to_string(),
        //     Operator::Eq,
        //     KeyType::StringKey50(FixedStringKey::from_str("NL.IMBAG.Pand.0503100000012869")),
        // ),
    ];

    let (_cj, features_count) = {
        let header = http_reader.header();
        println!("header: {header:?}");

        (to_cj_metadata(&header)?, header.features_count())
    };

    let mut iter = http_reader.select_attr_query(&query).await?;

    let mut features = Vec::new();
    let mut feat_num = 0;

    while let Some(feature) = iter.next().await? {
        let cj_feature = feature.cj_feature()?;
        features.push(cj_feature);
        feat_num += 1;
    }

    let feature = features.first().unwrap();
    let mut contains_b3_h_dak_50p = false;
    let _contains_identificatie = false;
    for co in feature.city_objects.values() {
        if co.attributes.is_some() {
            let attrs = co.attributes.as_ref().unwrap();
            if let Some(b3_h_dak_50p) = attrs.get("b3_h_dak_50p") {
                if b3_h_dak_50p.as_f64().unwrap() > 300.0 {
                    contains_b3_h_dak_50p = true;
                }
                if b3_h_dak_50p.as_f64().unwrap() < 300.0 {
                    contains_b3_h_dak_50p = false;
                }
            }
            // if let Some(identificatie) = attrs.get("identificatie") {
            //     if identificatie.as_str().unwrap() == "NL.IMBAG.Pand.0503100000012869" {
            //         contains_identificatie = true;
            //     }
            // }
        }
    }

    assert!(feat_num > 0 && feat_num < features_count);
    // assert!(contains_identificatie);
    assert!(contains_b3_h_dak_50p);
    Ok(())
}

mod http {
    use anyhow::Result;

    use fcb_core::packed_rtree::Query;
    use fcb_core::HttpFcbReader;

    use crate::{read_http_file_attr, read_http_file_bbox};

    // The published 3DBAG file (~68 GB, EPSG:28992). These tests hit the live
    // bucket, so they are `#[ignore]`d -- opt-in only. Run them with
    // `just test-remote` (or `cargo nextest run --run-ignored ignored-only`),
    // never in the default suite. FCB_REMOTE_HTTP_URL overrides the URL.
    //
    // The expected values were cross-checked across the Rust, C++, Python and
    // TypeScript readers on 2026-07-23; all four agree. Update all four suites
    // in lock-step if the file is regenerated (this one, test_http.cpp,
    // test_http.py, http.test.ts).
    const REMOTE_FEATURES_COUNT: u64 = 10_771_547;
    // A ~1 km box over central Amsterdam (minx, miny, maxx, maxy).
    const REMOTE_BBOX: (f64, f64, f64, f64) = (120_000.0, 486_000.0, 121_000.0, 487_000.0);
    const REMOTE_BBOX_COUNT: usize = 2762;

    fn remote_url() -> String {
        std::env::var("FCB_REMOTE_HTTP_URL").unwrap_or_else(|_| {
            "https://storage.googleapis.com/flatcitybuf/3dbag_all_index.fcb".to_string()
        })
    }

    #[tokio::test]
    #[ignore = "hits the live 68 GB 3DBAG bucket; run via `just test-remote`"]
    async fn remote_3dbag_opens_and_counts_a_bbox() -> Result<()> {
        let reader = HttpFcbReader::open(&remote_url()).await?;

        // The header verifies -> the file is in the post-alignment-fix format.
        assert_eq!(reader.header().features_count(), REMOTE_FEATURES_COUNT);

        // The R-tree yields the match count without materializing features,
        // so this is bounded regardless of the 68 GB body. Exact, and
        // identical to what C++, Python and TypeScript return for this box.
        let (minx, miny, maxx, maxy) = REMOTE_BBOX;
        let iter = reader
            .select_query(Query::BBox(minx, miny, maxx, maxy))
            .await?;
        assert_eq!(iter.features_count(), Some(REMOTE_BBOX_COUNT));
        Ok(())
    }

    #[tokio::test]
    #[ignore = "hits the live 68 GB 3DBAG bucket; run via `just test-remote`"]
    async fn remote_3dbag_bbox_scan() -> Result<()> {
        let res = read_http_file_bbox(&remote_url()).await;
        assert!(res.is_ok());
        Ok(())
    }

    #[tokio::test]
    #[ignore = "hits the live 68 GB 3DBAG bucket; run via `just test-remote`"]
    async fn remote_3dbag_attr_query() -> Result<()> {
        let res = read_http_file_attr(&remote_url()).await;
        assert!(res.is_ok());
        Ok(())
    }
}
