use anyhow::Result;
use fcb_core::{read_cityjson_from_reader, CJType, CJTypeKind, CityJSONSeq};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

/// NOTE: This file is only for temporary use.
fn read_cj(input: &str) -> Result<()> {
    let reader = BufReader::new(File::open(input)?);

    let cj_seq = match read_cityjson_from_reader(reader, CJTypeKind::Seq)? {
        CJType::Seq(seq) => seq,
        _ => anyhow::bail!("Expected CityJSONSeq"),
    };

    let CityJSONSeq { cj: _, features } = cj_seq;

    let feature_count = features.len() as u64;
    // get attribute "documentnummer" of features which located at every 10%th position of the features
    let mut document_nummers = Vec::new();
    let mut identifications = Vec::new();
    for (i, feature) in features.iter().enumerate() {
        if i as u64 % (feature_count / 10) == 0 {
            feature.city_objects.iter().for_each(|(_, co)| {
                if let Some(attributes) = &co.attributes {
                    if let Some(document_number) =
                        attributes.get("documentnummer").and_then(|v| v.as_str())
                    {
                        document_nummers.push(document_number.to_string());
                    }
                    if let Some(identification) =
                        attributes.get("identificatie").and_then(|v| v.as_str())
                    {
                        identifications.push(identification.to_string());
                    }
                }
            });
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input_file_path = manifest_dir
        .join("tests")
        .join("data")
        .join("delft.city.jsonl");
    let input_path = input_file_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid UTF-8 in path"))?;
    read_cj(input_path)?;
    Ok(())
}
