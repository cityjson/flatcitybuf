use crate::cjerror::CjError as Error;
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
    fn read_lines(&mut self) -> Box<dyn Iterator<Item = Result<String, Error>> + '_>;
}

impl<R: Read> CityJSONReader for BufReader<R> {
    fn read_lines(&mut self) -> Box<dyn Iterator<Item = Result<String, Error>> + '_> {
        Box::new(self.lines().map(|line| line.map_err(Error::Io)))
    }
}

impl CityJSONReader for &str {
    fn read_lines(&mut self) -> Box<dyn Iterator<Item = Result<String, Error>> + '_> {
        match std::fs::File::open(self) {
            Ok(file) => Box::new(
                BufReader::new(file)
                    .lines()
                    .map(|line| line.map_err(Error::Io)),
            ),
            Err(e) => Box::new(std::iter::once(Err(Error::Io(e)))),
        }
    }
}

fn parse_cityjson<T: CityJSONReader>(mut source: T, cj_type: CJTypeKind) -> Result<CJType, Error> {
    let mut lines = source.read_lines();

    match cj_type {
        CJTypeKind::Normal => {
            let content = lines.collect::<Result<Vec<_>, Error>>()?.join("\n");

            let cj: CityJSON = serde_json::from_str(&content)?;
            Ok(CJType::Normal(cj))
        }

        CJTypeKind::Seq => {
            // Read first line as CityJSON metadata
            let first_line = lines
                .next()
                .ok_or(Error::Io(std::io::Error::other("Empty input")))?;
            let cj: CityJSON = serde_json::from_str(&first_line?)?;

            // Read remaining lines as CityJSONFeatures
            let features: Result<Vec<_>, Error> = lines
                .map(|line| -> Result<_, Error> {
                    let line = line?;
                    Ok(serde_json::from_str(&line)?)
                })
                .collect();

            Ok(CJType::Seq(CityJSONSeq {
                cj,
                features: features?,
            }))
        }
    }
}

/// Read CityJSON from a file path
pub fn read_cityjson(file: &str, cj_type: CJTypeKind) -> Result<CJType, Error> {
    parse_cityjson(file, cj_type)
}

/// Read CityJSON from any reader (file or stdin)
pub fn read_cityjson_from_reader<R: Read>(
    reader: BufReader<R>,
    cj_type: CJTypeKind,
) -> Result<CJType, Error> {
    parse_cityjson(reader, cj_type)
}

/// Tests reading CityJSON data from a memory string
///
/// # Arguments
/// None
///
/// # Returns
/// * `Result<()>` - Ok if test passes, Error otherwise
///
/// # Example
/// ```
/// let test_data = include_str!("../tests/data/small.city.jsonl");
/// let result = read_cityjson(test_data, CJTypeKind::Seq)?;
/// ```
#[cfg(test)]
mod tests {
    use std::{fs::File, path::PathBuf};

    use super::*;

    #[test]
    fn test_read_from_memory() -> Result<(), Error> {
        let input_file = BufReader::new(File::open(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/small.city.jsonl"),
        )?);
        let result = read_cityjson_from_reader(input_file, CJTypeKind::Seq)?;

        if let CJType::Seq(seq) = result {
            assert_eq!(seq.features.len(), 3);
        } else {
            panic!("Expected Seq type");
        }

        Ok(())
    }
}
