//! CityGML 2.0 and 3.0, in and out of `.fcb`.
//!
//! No XML is read or written here. The `citygml2` and `citygml3` crates turn
//! a document into `cjseq` features and features back into a document; this
//! module chooses the transform the features are quantised against, spools
//! them so that `fcb ser` can pass over them more than once, and feeds an
//! `.fcb`'s features to a writer.
//!
//! `fcb ser` reads CityGML when its inputs end in `.gml` or `.xml`, and
//! `fcb deser` writes it when its output does.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use citygml_core::reader::{scan_preamble, Document, ObjectStream};
use citygml_core::Version;
use cityparquet_schema::crs::{axis_scale, resolve_to_projjson, MM};
use cjseq::{CityJSON, CityJSONFeature, Metadata, ReferenceSystem, Transform};
use fcb_core::{deserializer, FcbReader};

use crate::CliError;

/// Whether `path` names a CityGML document, by its extension (`.gml` or
/// `.xml`, in any case).
pub fn is_citygml_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("gml") || e.eq_ignore_ascii_case("xml"))
}

/// The CityGML version a document is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CityGmlVersion {
    #[default]
    V2_0,
    V3_0,
}

impl std::str::FromStr for CityGmlVersion {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "2.0" | "2" => Ok(Self::V2_0),
            "3.0" | "3" => Ok(Self::V3_0),
            other => Err(format!(
                "unsupported CityGML version {other} (expected 2.0 or 3.0)"
            )),
        }
    }
}

/// What reading the CityGML inputs found besides the features.
#[derive(Debug, Default)]
pub struct ReadReport {
    /// Members of a class the readers do not read, by element name.
    pub skipped_members: BTreeMap<String, usize>,
    /// Members of an ADE's class, read, by element name.
    pub ade_members: BTreeMap<String, usize>,
    /// ADE properties holding GML geometry, which the CityJSON conversion
    /// leaves out, by property name.
    pub ade_geometries: BTreeMap<String, usize>,
}

/// CityGML documents read as CityJSON: one header for all of them, and their
/// features, spooled.
pub struct CityGmlInput {
    pub header: CityJSON,
    pub features: Spool,
    pub report: ReadReport,
}

/// Features written to a temporary CityJSONSeq file, one per line, so that
/// they can be read more than once without holding them in memory.
pub struct Spool {
    file: File,
    len: usize,
    /// Subtracted from every spooled vertex as it is read back.
    offset: [i64; 3],
}

impl Spool {
    fn new() -> Result<Self, CliError> {
        Ok(Self {
            file: tempfile::tempfile()?,
            len: 0,
            offset: [0; 3],
        })
    }

    /// How many features the spool holds.
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The features, in the order they were read.
    pub fn iter(
        &self,
    ) -> Result<impl Iterator<Item = Result<CityJSONFeature, CliError>>, CliError> {
        let mut file = self.file.try_clone()?;
        file.seek(SeekFrom::Start(0))?;
        let offset = self.offset;
        Ok(BufReader::new(file).lines().map(move |line| {
            let line = line?;
            let mut feature = serde_json::from_str::<CityJSONFeature>(&line)?;
            if offset != [0; 3] {
                for v in &mut feature.vertices {
                    for (c, o) in v.iter_mut().zip(offset) {
                        *c -= o;
                    }
                }
            }
            Ok(feature)
        }))
    }
}

/// A CityGML document's error, as the CLI's.
fn gml_error(e: citygml_core::Error) -> CliError {
    CliError::CityGml(e.to_string())
}

/// The version of the CityGML document at `path`, which a reader exists for.
fn version_of(path: &Path) -> Result<CityGmlVersion, CliError> {
    match citygml_core::sniff(path) {
        Some(Version::V2_0) => Ok(CityGmlVersion::V2_0),
        Some(Version::V3_0) => Ok(CityGmlVersion::V3_0),
        Some(Version::Other(v)) => Err(CliError::CityGml(format!(
            "{}: unsupported CityGML version {v} (only 2.0 and 3.0 are read)",
            path.display()
        ))),
        None => Err(CliError::CityGml(format!(
            "{} is not a CityGML document",
            path.display()
        ))),
    }
}

/// Read `path` once for what its objects need from elsewhere in it.
fn read_document(path: &Path, version: CityGmlVersion) -> Result<Document, CliError> {
    match version {
        CityGmlVersion::V2_0 => citygml2::read_document(path),
        CityGmlVersion::V3_0 => citygml3::read_document(path),
    }
    .map_err(gml_error)
}

/// The objects of `path` as a stream, with `document` already read.
fn objects(
    path: &Path,
    version: CityGmlVersion,
    document: Arc<Document>,
) -> Result<Box<dyn ObjectStream + Send>, CliError> {
    Ok(match version {
        CityGmlVersion::V2_0 => {
            Box::new(citygml2::FeatureReader::with_document(path, document).map_err(gml_error)?)
        }
        CityGmlVersion::V3_0 => {
            Box::new(citygml3::FeatureReader::with_document(path, document).map_err(gml_error)?)
        }
    })
}

/// Read the CityGML documents at `paths` as one CityJSON dataset.
///
/// Every feature is quantised against one transform: `translate` is the
/// lower corner of the documents' envelopes together, and `scale` comes
/// from the units of their CRS (a degree axis gets a nano-degree step, a
/// metre axis a millimetre), the same rule cityparquet applies to the same
/// documents. When no document declares an envelope, `translate` is the
/// smallest coordinate the features hold instead, so that they fit the 32-bit
/// vertices of FlatCityBuf. The documents must declare the same CRS. Several documents
/// with implicit geometries cannot be combined, because an instance names
/// its template by its position in its own document.
pub fn read(paths: &[PathBuf]) -> Result<CityGmlInput, CliError> {
    let mut sources = Vec::with_capacity(paths.len());
    let mut srs: Option<(PathBuf, String)> = None;
    let mut envelope: Option<[f64; 6]> = None;
    for path in paths {
        let version = version_of(path)?;
        let preamble = scan_preamble(BufReader::new(File::open(path)?)).map_err(gml_error)?;
        if let Some(name) = preamble.srs_name {
            match &srs {
                Some((first, other)) if crs_code(other) != crs_code(&name) => {
                    return Err(CliError::CityGml(format!(
                        "{} declares the CRS {name}, but {} declares {other}; \
                         documents in different CRSs cannot be combined",
                        path.display(),
                        first.display()
                    )));
                }
                Some(_) => {}
                None => srs = Some((path.clone(), name)),
            }
        }
        if let Some(e) = preamble.envelope {
            envelope = Some(match envelope {
                None => e,
                Some(u) => [
                    u[0].min(e[0]),
                    u[1].min(e[1]),
                    u[2].min(e[2]),
                    u[3].max(e[3]),
                    u[4].max(e[4]),
                    u[5].max(e[5]),
                ],
            });
        }
        let document = Arc::new(read_document(path, version)?);
        sources.push((path.clone(), version, document));
    }

    let srs_name = srs.map(|(_, name)| name);
    let mut header = header(srs_name.as_deref(), envelope)?;

    let with_templates = sources
        .iter()
        .filter(|(_, _, d)| !d.templates().is_empty())
        .count();
    if with_templates > 0 && sources.len() > 1 {
        return Err(CityGmlError::TemplatesAcrossDocuments.into());
    }
    let mut extensions = BTreeMap::new();
    for (_, _, document) in &sources {
        if let Some((templates, appearance)) =
            citygml_core::convert::cjseq::geometry_templates(document.templates())
                .map_err(gml_error)?
        {
            header.geometry_templates = Some(templates);
            header.appearance = appearance;
        }
        extensions.extend(citygml_core::convert::cjseq::extensions(document.ade()));
    }
    if !extensions.is_empty() {
        header.extensions = Some(extensions.into_iter().collect());
    }

    let mut spool = Spool::new()?;
    let mut report = ReadReport::default();
    let mut lowest = [i64::MAX; 3];
    {
        let mut out = BufWriter::new(&spool.file);
        for (path, version, document) in sources {
            let mut features = citygml_core::reader::Features::new(
                objects(&path, version, document)?,
                header.transform.clone(),
            );
            for feature in features.by_ref() {
                let feature =
                    feature.map_err(|e| CliError::CityGml(format!("{}: {e}", path.display())))?;
                for v in &feature.vertices {
                    for (l, c) in lowest.iter_mut().zip(v) {
                        *l = (*l).min(*c);
                    }
                }
                serde_json::to_writer(&mut out, &feature)?;
                out.write_all(b"\n")?;
                spool.len += 1;
            }
            let reader = features.reader();
            add_counts(&mut report.skipped_members, reader.skipped_members());
            add_counts(&mut report.ade_members, reader.ade_members());
            add_counts(&mut report.ade_geometries, reader.ade_geometries());
        }
        out.flush()?;
    }
    // Without an envelope the transform was built with no translate; the
    // integers are shifted onto their own minimum, which loses nothing.
    if envelope.is_none() && lowest[0] != i64::MAX {
        spool.offset = lowest;
        let t = &mut header.transform;
        for ((translate, scale), l) in t.translate.iter_mut().zip(t.scale).zip(lowest) {
            *translate += l as f64 * scale;
        }
    }
    Ok(CityGmlInput {
        header,
        features: spool,
        report,
    })
}

fn add_counts(into: &mut BTreeMap<String, usize>, from: &BTreeMap<String, usize>) {
    for (name, n) in from {
        *into.entry(name.clone()).or_default() += n;
    }
}

/// Why CityGML input was refused.
#[derive(Debug, thiserror::Error)]
pub enum CityGmlError {
    #[error(
        "several CityGML documents with implicit geometries cannot be combined: \
         convert them one at a time"
    )]
    TemplatesAcrossDocuments,
}

impl From<CityGmlError> for CliError {
    fn from(e: CityGmlError) -> Self {
        CliError::CityGml(e.to_string())
    }
}

/// The CityJSON header for documents in `srs_name` within `envelope`: the
/// transform, the extent and the reference system.
fn header(srs_name: Option<&str>, envelope: Option<[f64; 6]>) -> Result<CityJSON, CliError> {
    let translate = match &envelope {
        Some(e) => [e[0], e[1], e[2]],
        None => [0.0, 0.0, 0.0],
    };
    let crs = srs_name.map(resolve_crs).unwrap_or(Crs::Unresolved);
    let key = match &crs {
        Crs::Epsg(code) => Some(code.clone()),
        Crs::Crs84 => Some("OGC:CRS84".to_string()),
        Crs::Unresolved => None,
    };
    // A CRS the PROJJSON table does not know keeps the millimetre step.
    let scale = match key.as_deref().and_then(|k| resolve_to_projjson(k).ok()) {
        Some(projjson) => axis_scale(&projjson).map_err(|e| CliError::CityGml(e.to_string()))?,
        None => [MM; 3],
    };

    let mut header = CityJSON::new();
    header.transform = Transform { scale, translate };
    let reference_system = match crs {
        Crs::Epsg(code) => Some(ReferenceSystem::new(
            None,
            "EPSG".to_string(),
            "0".to_string(),
            code,
        )),
        Crs::Crs84 => Some(ReferenceSystem::new(
            None,
            "OGC".to_string(),
            "1.3".to_string(),
            "CRS84".to_string(),
        )),
        Crs::Unresolved => None,
    };
    if envelope.is_some() || reference_system.is_some() {
        header.metadata = Some(Metadata {
            geographical_extent: envelope,
            identifier: None,
            point_of_contact: None,
            reference_date: None,
            reference_system,
            title: None,
            other: serde_json::Map::new(),
        });
    }
    Ok(header)
}

/// What an `srsName` names.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Crs {
    Epsg(String),
    /// `OGC:CRS84`, which has no EPSG code.
    Crs84,
    /// A name not understood: no CRS is advertised.
    Unresolved,
}

/// The CRS code of `srs_name`, for comparing two documents' declarations.
fn crs_code(srs_name: &str) -> Crs {
    resolve_crs(srs_name)
}

/// Resolve an `srsName`, by the rules cityparquet's CityGML reader applies:
/// the EPSG syntaxes, `CRS84`, and the German AdV URNs real CityGML 2.0
/// data uses.
fn resolve_crs(srs_name: &str) -> Crs {
    let name = srs_name.trim();
    if name.to_ascii_uppercase().contains("CRS84") {
        return Crs::Crs84;
    }
    if let Some(rest) = name.strip_prefix("urn:adv:crs:") {
        return match rest {
            "ETRS89_UTM32*DE_DHHN92_NH" => Crs::Epsg("5555".to_string()),
            "ETRS89_UTM33*DE_DHHN92_NH" => Crs::Epsg("5556".to_string()),
            "ETRS89_UTM32" => Crs::Epsg("25832".to_string()),
            "ETRS89_UTM33" => Crs::Epsg("25833".to_string()),
            _ => Crs::Unresolved,
        };
    }
    let tokens: Vec<&str> = name.split([':', '/']).filter(|t| !t.is_empty()).collect();
    if !tokens.iter().any(|t| t.eq_ignore_ascii_case("EPSG")) {
        return Crs::Unresolved;
    }
    tokens
        .iter()
        .rev()
        .find(|t| t.len() >= 4 && t.chars().all(|c| c.is_ascii_digit()))
        .map_or(Crs::Unresolved, |t| Crs::Epsg(t.to_string()))
}

/// What writing a CityGML document did.
pub struct WriteReport {
    /// Features written.
    pub features: usize,
    /// Top-level city objects written.
    pub objects: usize,
    /// What the target version could not hold, by a description, with
    /// counts.
    pub losses: Vec<(String, usize)>,
}

/// Write the features of the `.fcb` read from `input` as a CityGML document
/// of `version` to `out`, one feature at a time.
pub fn write<R: Read, W: Write>(
    input: R,
    out: W,
    version: CityGmlVersion,
) -> Result<WriteReport, CliError> {
    let mut reader = FcbReader::open(input)?.select_all_seq()?;
    let mut header = deserializer::to_cj_metadata(&reader.header())?;
    // `to_cj_metadata` reads an absent extent as all zeros, which the
    // writer would declare as the document's envelope.
    if reader.header().geographical_extent().is_none() {
        if let Some(metadata) = header.metadata.as_mut() {
            metadata.geographical_extent = None;
        }
    }
    let options = citygml_core::convert::cjseq::WriterOptions::default();
    let report = match version {
        CityGmlVersion::V2_0 => {
            let mut writer =
                citygml2::writer::Writer::new(out, &header, options).map_err(gml_error)?;
            while let Some(feature) = reader.next()? {
                writer
                    .write_feature(&feature.cur_cj_feature()?)
                    .map_err(gml_error)?;
            }
            let (mut out, report) = writer.finish().map_err(gml_error)?;
            out.flush()?;
            report
        }
        CityGmlVersion::V3_0 => {
            let mut writer =
                citygml3::writer::Writer::new(out, &header, options).map_err(gml_error)?;
            while let Some(feature) = reader.next()? {
                writer
                    .write_feature(&feature.cur_cj_feature()?)
                    .map_err(gml_error)?;
            }
            let (mut out, report) = writer.finish().map_err(gml_error)?;
            out.flush()?;
            report
        }
    };
    Ok(WriteReport {
        features: report.features,
        objects: report.objects,
        losses: report
            .losses
            .iter()
            .map(|(what, n)| (what.to_string(), n))
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srs_names_resolve_as_cityparquet_resolves_them() {
        assert_eq!(
            resolve_crs("http://www.opengis.net/def/crs/EPSG/0/6697"),
            Crs::Epsg("6697".into())
        );
        assert_eq!(
            resolve_crs("urn:ogc:def:crs:EPSG::28992"),
            Crs::Epsg("28992".into())
        );
        assert_eq!(resolve_crs("EPSG:7415"), Crs::Epsg("7415".into()));
        assert_eq!(
            resolve_crs("urn:adv:crs:ETRS89_UTM32*DE_DHHN92_NH"),
            Crs::Epsg("5555".into())
        );
        assert_eq!(resolve_crs("urn:ogc:def:crs:OGC:1.3:CRS84"), Crs::Crs84);
        assert_eq!(resolve_crs("not-EPSG:1"), Crs::Unresolved);
    }

    #[test]
    fn a_degree_crs_is_quantised_at_a_degree_step() {
        // EPSG:6697 (JGD2011 + height), what PLATEAU declares: a millimetre
        // step on a degree axis would quantise at about 100 m.
        let header = header(
            Some("http://www.opengis.net/def/crs/EPSG/0/6697"),
            Some([139.0, 35.0, 0.0, 139.1, 35.1, 50.0]),
        )
        .unwrap();
        assert_eq!(header.transform.scale, [1e-9, 1e-9, 1e-3]);
        assert_eq!(header.transform.translate, [139.0, 35.0, 0.0]);
    }
}
