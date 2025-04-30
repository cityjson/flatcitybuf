use std::error::Error;

use anyhow::Result;
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
use fcb_core::HttpFcbReader;
use fcb_core::{deserializer::to_cj_metadata, FixedStringKey, Float, KeyType, Operator};

async fn read_http_file_bbox(path: &str) -> Result<(), Box<dyn Error>> {
    let http_reader = HttpFcbReader::open(path).await?;
    let minx = 68989.19384501831;
    let miny = 444614.3991728433;
    let maxx = 70685.16687543111;
    let maxy = 446023.6031208569;

    let mut iter = http_reader.select_bbox(minx, miny, maxx, maxy).await?;
    let header = iter.header();
    let cj = to_cj_metadata(&header)?;

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
// async fn read_http_file_bbox(path: &str) -> Result<(), Box<dyn Error>> {
//     let http_reader = HttpFcbReader::open(path).await?;
//     let minx = 84227.77;
//     let miny = 445377.33;
//     let maxx = 85323.23;
//     let maxy = 446334.69;
//     let mut iter = http_reader.select_bbox(minx, miny, maxx, maxy).await?;
//     let header = iter.header();
//     let cj = to_cj_metadata(&header)?;

//     // let mut writer = BufWriter::new(File::create("delft_http.city.jsonl")?);
//     // writeln!(writer, "{}", serde_json::to_string(&cj)?)?;

//     let mut feat_num = 0;
//     let feat_count = header.features_count();
//     let mut features = Vec::new();
//     while let Some(feature) = iter.next().await? {
//         let cj_feature = feature.cj_feature()?;
//         features.push(cj_feature);
//         // writeln!(writer, "{}", serde_json::to_string(&cj_feature)?)?;

//         feat_num += 1;
//         if feat_num >= feat_count {
//             break;
//         }
//     }
//     println!("cj: {:?}", cj);
//     println!("features count: {:?}", features.len());
//     // TODO: add more tests
//     Ok(())
// }

async fn read_http_file_attr(path: &str) -> Result<(), Box<dyn Error>> {
    let http_reader = HttpFcbReader::open(path).await?;
    let query: Vec<(String, Operator, KeyType)> = vec![
        (
            "b3_h_dak_50p".to_string(),
            Operator::Gt,
            KeyType::Float64(Float(1.0)),
        ),
        (
            "identificatie".to_string(),
            Operator::Eq,
            KeyType::StringKey50(FixedStringKey::from_str("NL.IMBAG.Pand.0503100000012869")),
        ),
    ];

    let (cj, features_count) = {
        let header = http_reader.header();
        (to_cj_metadata(&header)?, header.features_count())
    };
    let mut iter = http_reader.select_attr_query(&query).await?;

    let mut features = Vec::new();
    let mut feat_num = 0;
    while let Ok(Some(feat_buf)) = iter.next().await {
        let feature = feat_buf.cj_feature()?;
        features.push(feature);
        feat_num += 1;
        if feat_num >= features_count {
            break;
        }
    }

    println!("deserialized_features: {:?}", features.len());

    let feature = features.first().unwrap();
    let contains_b3_h_dak_50p = false;
    let mut contains_identificatie = false;
    for co in feature.city_objects.values() {
        if co.attributes.is_some() {
            let attrs = co.attributes.as_ref().unwrap();
            // if let Some(b3_h_dak_50p) = attrs.get("b3_h_dak_50p") {
            //     if b3_h_dak_50p.as_f64().unwrap() > 10.0 {
            //         contains_b3_h_dak_50p = true;
            //     }
            //     if b3_h_dak_50p.as_f64().unwrap() < 10.0 {
            //         contains_b3_h_dak_50p = false;
            //     }
            // }
            if let Some(identificatie) = attrs.get("identificatie") {
                if identificatie.as_str().unwrap() == "NL.IMBAG.Pand.0503100000012869" {
                    contains_identificatie = true;
                }
            }
        }
    }

    assert!(feat_num > 0 && feat_num < features_count);
    assert!(contains_identificatie);
    Ok(())
}

mod http {
    use anyhow::Result;

    use crate::{read_http_file_attr, read_http_file_bbox};

    #[tokio::test]
    async fn test_read_http_file() -> Result<()> {
        let res =
            // read_http_file_bbox("https://storage.googleapis.com/flatcitybuf/3dbag_100k.fcb").await;
        read_http_file_bbox("https://storage.googleapis.com/flatcitybuf/delft_attr.fcb").await;

        assert!(res.is_ok());
        Ok(())
    }

    #[tokio::test]
    async fn test_read_http_file_attr() -> Result<()> {
        let _ = env_logger::builder().is_test(true).try_init();

        let res =
            read_http_file_attr("http://127.0.0.1:5501/src/rust/temp/3dbag_partial.fcb").await;
        // read_http_file_attr("https://storage.googleapis.com/flatcitybuf/delft_attr.fcb").await;
        assert!(res.is_ok());
        Ok(())
    }
}
