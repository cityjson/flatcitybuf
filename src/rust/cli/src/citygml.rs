//! CityGML 2.0 and 3.0, in and out of `.fcb`.
//!
//! No XML is read or written here. The `citygml` crate turns a document of
//! either version into a CityJSON header and `cjseq` features, and features
//! back into a document; this module combines several documents' headers, spools
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

use citygml::Version;
use cjseq::{CityJSON, CityJSONFeature};
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
fn gml_error(e: citygml::Error) -> CliError {
    CliError::CityGml(e.to_string())
}

/// The version of the CityGML document at `path`, which a reader exists for.
fn version_of(path: &Path) -> Result<Version, CliError> {
    match citygml::sniff(path) {
        Some(Version::Other(v)) => Err(CliError::CityGml(format!(
            "{}: unsupported CityGML version {v} (only 2.0 and 3.0 are read)",
            path.display()
        ))),
        Some(version) => Ok(version),
        None => Err(CliError::CityGml(format!(
            "{} is not a CityGML document",
            path.display()
        ))),
    }
}

/// Read the CityGML documents at `paths` as one CityJSON dataset.
///
/// One document's header is the one `citygml::header_with` builds for it,
/// the same cityparquet takes: `translate` is the lower corner of its
/// envelope and `scale` follows the units of its CRS (a nano-degree on a
/// degree axis, a millimetre on a metre axis). Several documents must
/// declare the same CRS, and share one transform from their envelopes
/// together. When no document declares an envelope, `translate` is the
/// smallest coordinate the features hold instead, so that they fit the
/// 32-bit vertices of FlatCityBuf. Several documents with implicit
/// geometries cannot be combined, because an instance names its template by
/// its position in its own document.
pub fn read(paths: &[PathBuf]) -> Result<CityGmlInput, CliError> {
    let mut sources = Vec::with_capacity(paths.len());
    let mut header: Option<CityJSON> = None;
    let mut extensions = BTreeMap::new();
    let mut with_templates = 0;
    for path in paths {
        let version = version_of(path)?;
        let document = citygml::read_document(path, &version).map_err(gml_error)?;
        let mut own = citygml::header_with(path, &document).map_err(gml_error)?;
        if !document.templates().is_empty() {
            with_templates += 1;
        }
        extensions.extend(own.extensions.take().into_iter().flatten());
        header = Some(match header {
            None => own,
            Some(combined) => combine(combined, own, &paths[0], path)?,
        });
        sources.push((path.clone(), version, Arc::new(document)));
    }
    if with_templates > 0 && sources.len() > 1 {
        return Err(CityGmlError::TemplatesAcrossDocuments.into());
    }
    let Some(mut header) = header else {
        return Err(CliError::CityGml("no CityGML document given".into()));
    };
    if !extensions.is_empty() {
        header.extensions = Some(extensions.into_iter().collect());
    }
    let has_envelope = extent(&header).is_some();

    let mut spool = Spool::new()?;
    let mut report = ReadReport::default();
    let mut lowest = [i64::MAX; 3];
    {
        let mut out = BufWriter::new(&spool.file);
        for (path, version, document) in sources {
            let mut features =
                citygml::FeatureReader::with_document(&path, &version, &header.transform, document)
                    .map_err(gml_error)?;
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
            let objects = features.objects();
            add_counts(&mut report.skipped_members, objects.skipped_members());
            add_counts(&mut report.ade_members, objects.ade_members());
            add_counts(&mut report.ade_geometries, objects.ade_geometries());
        }
        out.flush()?;
    }
    // Without an envelope the transform has no translate; the integers are
    // shifted onto their own minimum, which loses nothing.
    if !has_envelope && lowest[0] != i64::MAX {
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

/// A header's extent, when its document declares an envelope.
fn extent(header: &CityJSON) -> Option<[f64; 6]> {
    header.metadata.as_ref()?.geographical_extent
}

/// The header for the documents of `combined` and of `own` (the header of
/// `path`) together: one CRS, and one transform from their envelopes.
fn combine(
    mut combined: CityJSON,
    own: CityJSON,
    first: &Path,
    path: &Path,
) -> Result<CityJSON, CliError> {
    let crs = |h: &CityJSON| {
        h.metadata
            .as_ref()
            .and_then(|m| m.reference_system.as_ref())
            .and_then(|r| serde_json::to_string(r).ok())
    };
    if let (Some(a), Some(b)) = (crs(&combined), crs(&own)) {
        if a != b {
            return Err(CliError::CityGml(format!(
                "{} declares the CRS {b}, but {} declares {a}; \
                 documents in different CRSs cannot be combined",
                path.display(),
                first.display()
            )));
        }
    }
    if combined.transform.scale != own.transform.scale {
        return Err(CliError::CityGml(format!(
            "{} and {} are quantised at different steps; \
             documents in different CRSs cannot be combined",
            first.display(),
            path.display()
        )));
    }
    let union = match (extent(&combined), extent(&own)) {
        (Some(u), Some(e)) => Some(std::array::from_fn(|i| {
            if i < 3 {
                u[i].min(e[i])
            } else {
                u[i].max(e[i])
            }
        })),
        (u, e) => u.or(e),
    };
    if let Some(u) = union {
        combined.transform.translate = [u[0], u[1], u[2]];
    }
    match (combined.metadata.as_mut(), own.metadata) {
        (Some(m), _) => m.geographical_extent = union,
        (None, Some(mut m)) => {
            m.geographical_extent = union;
            combined.metadata = Some(m);
        }
        (None, None) => {}
    }
    Ok(combined)
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
    let version = match version {
        CityGmlVersion::V2_0 => Version::V2_0,
        CityGmlVersion::V3_0 => Version::V3_0,
    };
    let mut writer =
        citygml::Writer::new(&version, out, &header, citygml::WriterOptions::default())
            .map_err(gml_error)?;
    while let Some(feature) = reader.next()? {
        writer
            .write_feature(&feature.cur_cj_feature()?)
            .map_err(gml_error)?;
    }
    let (mut out, report) = writer.finish().map_err(gml_error)?;
    out.flush()?;
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
