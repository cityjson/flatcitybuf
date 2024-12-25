use anyhow::anyhow;
use anyhow::Result;
use cjseq::{CityJSON, CityJSONFeature};
use std::fs::File;
use std::io::{BufRead, BufReader};

pub trait CityJSONSource {
    fn read_lines(&mut self) -> std::io::Lines<impl BufRead>;
}

impl CityJSONSource for BufReader<File> {
    fn read_lines(&mut self) -> std::io::Lines<impl BufRead> {
        self.lines()
    }
}

impl CityJSONSource for &str {
    fn read_lines(&mut self) -> std::io::Lines<impl BufRead> {
        let file = File::open(self).expect("Failed to open file");
        BufReader::new(file).lines()
    }
}

pub struct CityJSONSeq {
    pub cj: CityJSON,
    pub features: Vec<CityJSONFeature>, // TODO: use iterator for performance reason
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

fn parse_cityjson<T: CityJSONSource>(mut source: T, cj_type: CJTypeKind) -> Result<CJType> {
    let mut lines = source.read_lines().enumerate();

    let (_, first_line) = lines.next().ok_or_else(|| anyhow!("Empty file"))?;
    let mut cjj: CityJSON = serde_json::from_str(&first_line.unwrap())?;

    match cj_type {
        CJTypeKind::Normal => {
            for (_, line) in lines {
                let mut feature: CityJSONFeature = serde_json::from_str(&line?)?;
                cjj.add_cjfeature(&mut feature);
            }
            cjj.remove_duplicate_vertices();
            Ok(CJType::Normal(cjj))
        }

        CJTypeKind::Seq => {
            let features: Result<Vec<_>> = lines
                .map(|(_, line)| -> Result<_> {
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

pub fn read_cityjson(file: &str, cj_type: CJTypeKind) -> Result<CJType> {
    parse_cityjson(file, cj_type)
}

pub fn read_cityjson_from_bufreader(
    reader: BufReader<File>,
    cj_type: CJTypeKind,
) -> Result<CJType> {
    parse_cityjson(reader, cj_type)
}
