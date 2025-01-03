use anyhow::{anyhow, Result};
use cjseq::{CityJSON, CityJSONFeature};
use std::io::{BufRead, BufReader, Read};

pub struct CityJSONSeq {
    pub cj: CityJSON,
    pub features: Vec<CityJSONFeature>,
}

pub enum CJType {
    Normal(CityJSON),
    Seq(CityJSONSeq),
}

#[derive(Debug)]
pub enum CJTypeKind {
    Normal,
    Seq,
}

pub trait CityJSONReader {
    fn read_lines(&mut self) -> Box<dyn Iterator<Item = Result<String>> + '_>;
}

impl<R: Read> CityJSONReader for BufReader<R> {
    fn read_lines(&mut self) -> Box<dyn Iterator<Item = Result<String>> + '_> {
        Box::new(self.lines().map(|line| line.map_err(anyhow::Error::from)))
    }
}

impl CityJSONReader for &str {
    fn read_lines(&mut self) -> Box<dyn Iterator<Item = Result<String>> + '_> {
        match std::fs::File::open(self) {
            Ok(file) => Box::new(
                BufReader::new(file)
                    .lines()
                    .map(|line| line.map_err(anyhow::Error::from)),
            ),
            Err(e) => Box::new(std::iter::once(Err(anyhow::Error::from(e)))),
        }
    }
}

fn parse_cityjson<T: CityJSONReader>(mut source: T, cj_type: CJTypeKind) -> Result<CJType> {
    let mut lines = source.read_lines();

    let first_line = lines.next().ok_or_else(|| anyhow!("Empty input"))??;

    let mut cjj: CityJSON = serde_json::from_str(&first_line)?;

    match cj_type {
        CJTypeKind::Normal => {
            for line in lines {
                let mut feature: CityJSONFeature = serde_json::from_str(&line?)?;
                cjj.add_cjfeature(&mut feature);
            }
            cjj.remove_duplicate_vertices();
            Ok(CJType::Normal(cjj))
        }

        CJTypeKind::Seq => {
            let features: Result<Vec<_>> = lines
                .map(|line| -> Result<_> {
                    let line = line?;
                    Ok(serde_json::from_str(&line)?)
                })
                .collect();

            Ok(CJType::Seq(CityJSONSeq {
                cj: cjj,
                features: features?,
            }))
        }
    }
}

/// Read CityJSON from a file path
pub fn read_cityjson(file: &str, cj_type: CJTypeKind) -> Result<CJType> {
    parse_cityjson(file, cj_type)
}

/// Read CityJSON from any reader (file or stdin)
pub fn read_cityjson_from_reader<R: Read>(
    reader: BufReader<R>,
    cj_type: CJTypeKind,
) -> Result<CJType> {
    parse_cityjson(reader, cj_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_read_from_memory() -> Result<()> {
        let data = r#"{"type":"CityJSON","version":"1.1"}
{"type":"CityJSONFeature","id":"feature1"}
{"type":"CityJSONFeature","id":"feature2"}"#;

        let reader = BufReader::new(Cursor::new(data));
        let result = read_cityjson_from_reader(reader, CJTypeKind::Seq)?;

        if let CJType::Seq(seq) = result {
            assert_eq!(seq.features.len(), 2);
            assert_eq!(seq.features[0].id, "feature1");
            assert_eq!(seq.features[1].id, "feature2");
        } else {
            panic!("Expected Seq type");
        }

        Ok(())
    }
}
