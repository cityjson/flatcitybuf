use anyhow::Result;
use fcb_core::FcbReader;
use std::{fs::File, io::BufReader, path::PathBuf};

#[test]
fn read_bbox() -> Result<()> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input_file = manifest_dir
        .join("tests")
        .join("data")
        .join("delft_bbox.fcb");
    let mut filein = BufReader::new(File::open(input_file.clone())?);

    let minx = -200000.0;
    let miny = -200000.0;
    let maxx = 200000.0;
    let maxy = 200000.0;

    let mut fcb = FcbReader::open(&mut filein)?.select_bbox(minx, miny, maxx, maxy)?;

    assert_ne!(fcb.features_count(), None);

    let mut features = Vec::new();
    let mut bbox_cnt = 0;
    while let Some(feature) = fcb.next()? {
        bbox_cnt += 1;
        let cj_feat = feature.cur_cj_feature()?;
        features.push(cj_feat);
    }

    println!("bbox_cnt: {}", bbox_cnt);
    println!(
        "fcb.header().features_count(): {}",
        fcb.header().features_count()
    );

    assert!(bbox_cnt < fcb.header().features_count());

    let mut count_to_check = 0;
    for feature in features {
        let x_s = feature.vertices.iter().map(|v| v[0]).collect::<Vec<_>>();
        let y_s = feature.vertices.iter().map(|v| v[1]).collect::<Vec<_>>();

        // MEMO: it retrieves all features which has intersection with the bbox
        if x_s.iter().any(|x| *x >= minx as i64)
            || y_s.iter().any(|y| *y >= miny as i64)
            || x_s.iter().any(|x| *x <= maxx as i64)
            || y_s.iter().any(|y| *y <= maxy as i64)
        {
            count_to_check += 1;
        }
    }
    assert_eq!(count_to_check, bbox_cnt);

    Ok(())
}
