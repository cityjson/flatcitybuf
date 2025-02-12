use std::error::Error;

use fcb_core::{deserializer::to_cj_metadata, HttpFcbReader};

use anyhow::Result;
async fn read_http_file(path: &str) -> Result<(), Box<dyn Error>> {
    let http_reader = HttpFcbReader::open(path).await?;
    let minx = 84227.77;
    let miny = 445377.33;
    let maxx = 85323.23;
    let maxy = 446334.69;
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
    println!("cj: {:?}", cj);
    println!("features: {:?}", features);
    // TODO: add more tests
    Ok(())
}

mod http {
    use anyhow::Result;

    use crate::read_http_file;

    #[tokio::test]
    async fn test_read_http_file() -> Result<()> {
        let res = read_http_file("https://storage.googleapis.com/flatcitybuf/delft_bbox.fcb").await;
        assert!(res.is_ok());
        Ok(())
    }
}
