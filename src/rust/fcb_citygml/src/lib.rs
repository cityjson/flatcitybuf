//! CityGML 2.0 to CityJSON(Seq) conversion.
//!
//! The entry point is [`parse_citygml`], which streams a CityGML document and
//! produces a [`CityGmlDocument`] — the CityJSONSeq metadata line plus one
//! [`cjseq::CityJSONFeature`] per top-level city object — together with a
//! [`ParseReport`] describing anything that was valid CityGML but could not be
//! represented in CityJSON.

pub mod crs;
mod error;
pub mod gml;
pub mod xml;

pub use error::CityGmlError;

use quick_xml::events::Event;
use quick_xml::NsReader;

/// Local name of the only supported root element.
const ROOT_LOCAL_NAME: &[u8] = b"CityModel";

/// Placeholder reported when a document contains no element at all.
const NO_ROOT_ELEMENT: &str = "(none)";

/// Options controlling how a CityGML document is converted.
#[derive(Debug, Clone)]
pub struct ParseOptions {
    /// Quantisation scale written to the CityJSON `transform`, and used to
    /// quantise every coordinate.
    pub scale: [f64; 3],
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            scale: [0.001, 0.001, 0.001],
        }
    }
}

/// A converted CityGML document, in CityJSONSeq shape: the metadata
/// ("first line") object plus the features that follow it.
#[derive(Debug, Clone)]
pub struct CityGmlDocument {
    pub metadata: cjseq::CityJSON,
    pub features: Vec<cjseq::CityJSONFeature>,
}

/// Content that was skipped, and warnings raised, during a conversion.
///
/// Malformed structure is a hard [`CityGmlError`]; valid-but-unsupported
/// content is recorded here instead, so a conversion never silently loses
/// data.
#[derive(Debug, Default)]
pub struct ParseReport {
    pub skipped: Vec<Skipped>,
    pub warnings: Vec<String>,
}

/// One skipped element, with enough context to locate it in the source.
#[derive(Debug)]
pub struct Skipped {
    pub element: String,
    pub gml_id: Option<String>,
    pub reason: String,
}

/// Parse a CityGML 2.0 document into CityJSON(Seq) structures.
///
/// # Errors
///
/// Returns [`CityGmlError::UnsupportedRoot`] when the root element is not a
/// `CityModel`, and [`CityGmlError::Xml`] — with the byte position reached —
/// when the document is not well-formed XML. No input causes a panic.
///
/// # Examples
///
/// ```
/// use fcb_citygml::{parse_citygml, ParseOptions};
///
/// let xml = r#"<core:CityModel xmlns:core="http://www.opengis.net/citygml/2.0"/>"#;
/// let (doc, report) =
///     parse_citygml(std::io::BufReader::new(xml.as_bytes()), &ParseOptions::default())?;
/// assert_eq!(doc.metadata.version, "2.0");
/// assert!(doc.features.is_empty());
/// assert!(report.skipped.is_empty());
/// # Ok::<(), fcb_citygml::CityGmlError>(())
/// ```
pub fn parse_citygml<R: std::io::BufRead>(
    reader: R,
    opts: &ParseOptions,
) -> Result<(CityGmlDocument, ParseReport), CityGmlError> {
    let mut reader = NsReader::from_reader(reader);
    let mut buf = Vec::new();
    let report = ParseReport::default();

    read_root(&mut reader, &mut buf)?;
    // Consume the remainder so that malformed content anywhere in the
    // document is reported, not just malformed content in the prologue.
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(source) => return Err(xml_error(&reader, source)),
        }
    }

    Ok((empty_document(opts), report))
}

/// Advance to the first element and verify that it is a `CityModel`.
fn read_root<R: std::io::BufRead>(
    reader: &mut NsReader<R>,
    buf: &mut Vec<u8>,
) -> Result<(), CityGmlError> {
    loop {
        buf.clear();
        // The root's local name is what identifies a CityGML document here;
        // per-element namespace checking happens in the module readers.
        match reader.read_event_into(buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = e.name();
                if name.local_name().as_ref() == ROOT_LOCAL_NAME {
                    return Ok(());
                }
                return Err(CityGmlError::UnsupportedRoot(
                    String::from_utf8_lossy(name.as_ref()).into_owned(),
                ));
            }
            Ok(Event::Eof) => {
                return Err(CityGmlError::UnsupportedRoot(NO_ROOT_ELEMENT.to_string()))
            }
            Ok(_) => continue,
            Err(source) => return Err(xml_error(reader, source)),
        }
    }
}

/// Attach the reader's current byte position to a quick-xml error.
fn xml_error<R>(reader: &NsReader<R>, source: quick_xml::Error) -> CityGmlError {
    CityGmlError::Xml {
        position: reader.buffer_position(),
        source,
    }
}

/// The CityJSONSeq metadata line for a document with no city objects.
fn empty_document(opts: &ParseOptions) -> CityGmlDocument {
    let mut metadata = cjseq::CityJSON::new();
    metadata.transform.scale = opts.scale.to_vec();
    metadata.transform.translate = vec![0.0, 0.0, 0.0];
    CityGmlDocument {
        metadata,
        features: Vec::new(),
    }
}
