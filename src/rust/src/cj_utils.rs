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

fn parse_cityjson<T: CityJSONSource>(
    mut source: T,
) -> Result<CityJSON, Box<dyn std::error::Error>> {
    let mut cjj: CityJSON = CityJSON::new();
    for (i, line) in source.read_lines().enumerate() {
        match line {
            Ok(line) => {
                if i == 0 {
                    cjj = serde_json::from_str(&line)?;
                } else {
                    let mut cjf: CityJSONFeature = serde_json::from_str(&line)?;
                    cjj.add_cjfeature(&mut cjf);
                }
            }
            Err(e) => return Err(e.into()),
        }
        cjj.remove_duplicate_vertices();
    }
    Ok(cjj)
}

pub fn read_cityjson(file: &str) -> Result<CityJSON, Box<dyn std::error::Error>> {
    parse_cityjson(file)
}

pub fn read_cityjson_from_bufreader(
    reader: BufReader<File>,
) -> Result<CityJSON, Box<dyn std::error::Error>> {
    parse_cityjson(reader)
}
