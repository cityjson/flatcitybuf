use flatcitybuf::deserializer::to_cj_metadata;
use flatcitybuf::FcbReader;
use std::error::Error;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::PathBuf;

fn read_file() -> Result<(), Box<dyn Error>> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input_file_path = manifest_dir.join("temp").join("test_output.fcb");
    let input_file = File::open(input_file_path)?;
    let inputreader = BufReader::new(input_file);

    let output_file = manifest_dir
        .join("temp")
        .join("test_output_header.city.jsonl");
    let output_file = File::create(output_file)?;
    let mut outputwriter = BufWriter::new(output_file);

    let mut reader = FcbReader::open(inputreader)?.select_all()?;
    let header = reader.header();
    let cj = to_cj_metadata(&header)?;
    let mut features = Vec::new();
    let feat_count = header.features_count();
    let mut feat_num = 0;
    while let Some(feat_buf) = reader.next()? {
        let feature = feat_buf.cur_cj_feature()?;
        if feat_num == 0 {
            println!("feature: {:?}", feature);
        }
        features.push(feature);
        feat_num += 1;
        if feat_num >= feat_count {
            break;
        }
    }

    outputwriter.write_all(format!("{}\n", serde_json::to_string(&cj).unwrap()).as_bytes())?;

    for feature in &features {
        outputwriter
            .write_all(format!("{}\n", serde_json::to_string(feature).unwrap()).as_bytes())?;
    }

    // let original_cjseq_file = manifest_dir
    //     .join("tests")
    //     .join("data")
    //     .join("small.city.jsonl");
    // let cjseq_file = File::open(original_cjseq_file)?;
    // let cjseq_reader = BufReader::new(cjseq_file);
    // let cj_seq = read_cityjson_from_reader(cjseq_reader, CJTypeKind::Seq)?;
    // if let CJType::Seq(cj_seq) = cj_seq {
    //     let CityJSONSeq {
    //         cj: original_cj,
    //         features: original_features,
    //     } = cj_seq;

    //     // Compare features
    //     if original_features != features {
    //         println!("features differ:");
    //         if original_features.len() != features.len() {
    //             println!(
    //                 "length mismatch: {} != {}",
    //                 original_features.len(),
    //                 features.len()
    //             );
    //         } else {
    //             for (i, (orig, new)) in original_features.iter().zip(features.iter()).enumerate() {
    //                 if orig != new {
    //                     println!("feature {} differs:", i);
    //                     if orig.thetype != new.thetype {
    //                         println!("  type mismatch: {} != {}", orig.thetype, new.thetype);
    //                     }
    //                     if orig.city_objects != new.city_objects {
    //                         println!("  city_objects mismatch at index {}", i);
    //                         //compare the first element of the city_objects
    //                         let orig_first = orig.city_objects.iter().next().unwrap();
    //                         let new_first = new.city_objects.get(orig_first.0).unwrap();

    //                         println!("  key mismatch: {}", orig_first.0);
    //                         if orig_first.1.geometry != new_first.geometry {
    //                             println!(
    //                                 "  geometry mismatch: {:?} != {:?}",
    //                                 orig_first.1.geometry, new_first.geometry
    //                             );
    //                             //geometry is a vector of geometry, iterate over the elements and compare
    //                             for (i, (orig_geom, new_geom)) in orig_first
    //                                 .1
    //                                 .geometry
    //                                 .iter()
    //                                 .zip(new_first.geometry.iter())
    //                                 .enumerate()
    //                             {
    //                                 if orig_geom != new_geom {
    //                                     println!(
    //                                         "  geometry element {} mismatch: {:?} != {:?}",
    //                                         i, orig_geom, new_geom
    //                                     );
    //                                 }
    //                                 // compare each element of the geometry
    //                                 for (j, (orig_elem, new_elem)) in
    //                                     orig_geom.iter().zip(new_geom.iter()).enumerate()
    //                                 {
    //                                     if orig_elem != new_elem {
    //                                         println!(
    //                                             "  geometry element {} mismatch: {:?} != {:?}",
    //                                             i, orig_elem, new_elem
    //                                         );
    //                                     }
    //                                     if orig_elem.boundaries != new_elem.boundaries {
    //                                         println!(
    //                                             "  boundaries mismatch: {:?} != {:?}",
    //                                             orig_elem.boundaries, new_elem.boundaries
    //                                         );
    //                                     }
    //                                     if orig_elem.thetype != new_elem.thetype {
    //                                         println!(
    //                                             "  type mismatch: {:?} != {:?}",
    //                                             orig_elem.thetype, new_elem.thetype
    //                                         );
    //                                     }
    //                                     if orig_elem.lod != new_elem.lod {
    //                                         println!(
    //                                             "  lod mismatch: {:?} != {:?}",
    //                                             orig_elem.lod, new_elem.lod
    //                                         );
    //                                     }
    //                                 }
    //                             }
    //                         }
    //                         if orig_first.1.geographical_extent != new_first.geographical_extent {
    //                             println!(
    //                                 "  geographical_extent mismatch: {:?} != {:?}",
    //                                 orig_first.1.geographical_extent, new_first.geographical_extent
    //                             );
    //                         }
    //                     }
    //                     if orig.vertices != new.vertices {
    //                         println!(
    //                             "  vertices mismatch: {:?} != {:?}",
    //                             orig.vertices, new.vertices
    //                         );
    //                     }
    //                 }
    //             }
    //         }
    //         panic!("features are not equal");
    //     }
    // }

    Ok(())
}

fn main() {
    read_file().unwrap();
}
