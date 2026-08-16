//! CityGML 2.0 to CityJSON(Seq) conversion.
//!
//! The entry point is [`parse_citygml`], which streams a CityGML document and
//! produces a [`CityGmlDocument`] — the CityJSONSeq metadata line plus one
//! [`cjseq::CityJSONFeature`] per top-level city object — together with a
//! [`ParseReport`] describing anything that was valid CityGML but could not be
//! represented in CityJSON.
//!
//! Reading and converting are two halves. The first streams the document,
//! buffers one `cityObjectMember` subtree at a time, and hands each to a
//! module reader, which builds the [`IntermediateObject`] tree in real-world
//! coordinates; [`parse_to_model`] exposes that half on its own. Only the
//! second half — the converter — quantises, which it cannot do earlier: the
//! transform is not known until the last coordinate has been seen. Appearance
//! crosses the two: the surface data is collected by the reader and joined to
//! the polygons it paints by the converter, which is the half that has them.
//!
//! The public interface is [`parse_citygml`], [`ParseOptions`] and what they
//! answer with — [`CityGmlDocument`], [`ParseReport`], [`Skipped`] and
//! [`CityGmlError`]. Everything else is this crate's own: the XML tree, the
//! GML readers and the intermediate model are reachable only so that
//! [`parse_to_model`] can be tested against them, and none of them is a
//! stable interface.

pub(crate) mod appearance;
mod citygml;
mod convert;
#[doc(hidden)]
pub mod crs;
mod error;
#[doc(hidden)]
pub mod gml;
#[doc(hidden)]
pub mod model;
pub(crate) mod xml;

use convert::convert;
pub use error::CityGmlError;
#[doc(hidden)]
pub use model::{IntermediateGeometry, IntermediateObject, SemanticSurface};

use std::io::BufRead;

use quick_xml::events::Event;
use quick_xml::NsReader;

use crate::appearance::parse_appearances;
use crate::crs::NormalizedCrs;
use crate::gml::{gml_child, is_gml, GML_NS, SRS_DIMENSION_ATTR};
use crate::xml::XmlNode;

/// Local name of the only supported root element.
const ROOT_LOCAL_NAME: &[u8] = b"CityModel";

/// Placeholder reported when a document contains no element at all.
const NO_ROOT_ELEMENT: &str = "(none)";

/// Namespace URIs of the CityGML core module, 2.0 and 1.0. Both are accepted
/// throughout: the modules differ only in ways this converter does not read,
/// and files written against 1.0 are still in circulation.
const CORE_NS: [&str; 2] = [
    "http://www.opengis.net/citygml/2.0",
    "http://www.opengis.net/citygml/1.0",
];

/// Namespace URIs of the CityGML appearance module, 2.0 and 1.0.
const APPEARANCE_NS: [&str; 2] = [
    "http://www.opengis.net/citygml/appearance/2.0",
    "http://www.opengis.net/citygml/appearance/1.0",
];

/// Local names of the `CityModel` properties this scan recognises.
const CITY_OBJECT_MEMBER: &str = "cityObjectMember";
const APPEARANCE_MEMBER: &str = "appearanceMember";
const BOUNDED_BY: &str = "boundedBy";
const ENVELOPE: &str = "Envelope";

/// Local name of the appearance element a city object carries its own
/// appearance in. CityGML declares appearance in two places — as an
/// `app:appearanceMember` of the `CityModel` and as an `app:appearance` of a
/// city object — and the two mean the same thing, so both are collected.
const APPEARANCE: &str = "Appearance";

/// Local name of the GML attribute naming a coordinate reference system.
const SRS_NAME_ATTR: &str = "srsName";

/// The warning raised when a document names no CRS anywhere.
const NO_SRS_NAME: &str = "no srsName found; referenceSystem omitted";

/// Coordinates per position, which is the only shape CityJSON holds.
const DIMS: usize = 3;

/// Options controlling how a CityGML document is converted.
#[derive(Debug, Clone)]
pub struct ParseOptions {
    /// Quantisation scale written to the CityJSON `transform`, and used to
    /// quantise every coordinate.
    pub scale: [f64; 3],
    /// What to call a top-level city object whose `gml:id` is missing: this
    /// prefix, then the position of its `cityObjectMember` in the document.
    ///
    /// `None` is [`DEFAULT_ID_PREFIX`], which is unique within one document
    /// and only within one. A caller that converts several documents into one
    /// dataset — as `fcb ser` does — should pass something that names the
    /// source, its file stem for choice, or the second file's
    /// `citygml-obj-0` will collide with the first's.
    pub id_prefix: Option<String>,
}

/// The prefix a generated object id carries when the caller names none.
pub const DEFAULT_ID_PREFIX: &str = "citygml-obj";

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            scale: [0.001, 0.001, 0.001],
            id_prefix: None,
        }
    }
}

impl ParseOptions {
    /// The prefix generated ids are built from.
    fn id_prefix(&self) -> &str {
        self.id_prefix.as_deref().unwrap_or(DEFAULT_ID_PREFIX)
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
pub fn parse_citygml<R: BufRead>(
    reader: R,
    opts: &ParseOptions,
) -> Result<(CityGmlDocument, ParseReport), CityGmlError> {
    let (objects, crs, appearances, mut report) = scan_city_model(reader, opts)?;
    let appearances = parse_appearances(&appearances, &mut report);
    let document = convert(objects, crs, appearances, opts, &mut report);
    Ok((document, report))
}

/// Read a CityGML document into the intermediate model, without converting
/// it.
///
/// This is the reading half of [`parse_citygml`], exposed so that the reader
/// can be tested on the model it builds rather than only on the CityJSON that
/// comes out the far end. It is not a stable interface.
///
/// Appearance is deliberately not part of what comes back, which is why the
/// signature is unchanged now that appearance is read. The intermediate model
/// describes city objects, and a CityGML appearance is not one: it is stated
/// beside them and is joined to their polygons by the converter alone. A
/// caller that wants the materials wants [`parse_citygml`].
///
/// # Errors
///
/// As [`parse_citygml`], plus the module readers' errors: malformed geometry,
/// and `xlink:href`s that name nothing in the member that carries them.
#[doc(hidden)]
pub fn parse_to_model<R: BufRead>(
    reader: R,
    opts: &ParseOptions,
) -> Result<(Vec<IntermediateObject>, Option<NormalizedCrs>, ParseReport), CityGmlError> {
    let (objects, crs, _appearances, report) = scan_city_model(reader, opts)?;
    Ok((objects, crs, report))
}

/// Everything one pass over a `CityModel` yields.
type ScannedModel = (
    Vec<IntermediateObject>,
    Option<NormalizedCrs>,
    Vec<XmlNode>,
    ParseReport,
);

/// Stream a `CityModel`, one top-level property at a time.
///
/// Only a single property's subtree is in memory at once, so a document
/// costs its largest city object rather than its whole self.
fn scan_city_model<R: BufRead>(
    reader: R,
    opts: &ParseOptions,
) -> Result<ScannedModel, CityGmlError> {
    let mut reader = NsReader::from_reader(reader);
    let mut buf = Vec::new();
    read_root(&mut reader, &mut buf)?;

    let mut scan = Scan {
        id_prefix: opts.id_prefix().to_string(),
        ..Scan::default()
    };
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let start = e.into_owned();
                let node = xml::load_subtree(&mut reader, start)?;
                scan.read_property(node)?;
            }
            Ok(Event::Empty(e)) => {
                let node = xml::leaf_node(&reader, &e)?;
                scan.read_property(node)?;
            }
            // The document element's end tag: every deeper one was consumed
            // along with its subtree.
            Ok(Event::End(_)) | Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(source) => return Err(xml_error(&reader, source)),
        }
    }
    // Consume the remainder so that malformed content after the document
    // element is reported rather than ignored.
    drain_to_eof(&mut reader, &mut buf)?;

    Ok(scan.finish())
}

/// The state a scan accumulates as the document streams past it.
#[derive(Default)]
struct Scan {
    objects: Vec<IntermediateObject>,
    /// What a generated object id starts with; see [`ParseOptions`].
    id_prefix: String,
    /// The `srsName` of the first top-level `gml:Envelope` that carries one.
    envelope_srs: Option<String>,
    /// The `srsName` of the first GML element inside a member that carries
    /// one, used only when no Envelope named a CRS.
    geometry_srs: Option<String>,
    /// Appearance subtrees, kept whole for the appearance reader: the
    /// `app:appearanceMember`s of the `CityModel`, and every `app:Appearance`
    /// found inside a member, in document order.
    appearances: Vec<XmlNode>,
    /// How many `cityObjectMember`s have been seen, including skipped ones,
    /// so that a generated object id stays stable for a given document.
    members: usize,
    report: ParseReport,
}

impl Scan {
    /// Take in one top-level property of the `CityModel`.
    fn read_property(&mut self, node: XmlNode) -> Result<(), CityGmlError> {
        if is_in(&node, &CORE_NS, CITY_OBJECT_MEMBER) {
            let index = self.members;
            self.members += 1;
            if self.geometry_srs.is_none() {
                self.geometry_srs = geometry_srs_name(&node);
            }
            // A city object may carry its own `app:appearance`, which means
            // exactly what an `app:appearanceMember` of the CityModel means:
            // its targets are `gml:id`s, and nothing scopes them to the
            // object. So they join the document's appearances rather than
            // travelling with the object, and the search is over descendants
            // because the property may sit on a nested object — a building
            // part, a boundary surface — as readily as on the root one.
            self.appearances.extend(
                node.descendants()
                    .filter(|node| is_in(node, &APPEARANCE_NS, APPEARANCE))
                    .cloned(),
            );
            // One member usually yields one object, but not always: the
            // components of a `dem:ReliefFeature` each become a top-level
            // object of their own. See [`citygml::read_member`].
            let member_id = citygml::MemberId {
                prefix: &self.id_prefix,
                index,
            };
            self.objects
                .extend(citygml::read_member(&node, member_id, &mut self.report)?);
            return Ok(());
        }

        if is_gml(&node, BOUNDED_BY) {
            let envelope = gml_child(&node, ENVELOPE);
            if self.envelope_srs.is_none() {
                self.envelope_srs = envelope
                    .and_then(|envelope| envelope.attr(SRS_NAME_ATTR))
                    .map(str::to_owned);
            }
            if let Some(envelope) = envelope {
                self.check_envelope_dimension(envelope);
            }
            return Ok(());
        }

        if is_in(&node, &APPEARANCE_NS, APPEARANCE_MEMBER) {
            self.appearances.push(node);
            return Ok(());
        }

        self.report.skipped.push(Skipped {
            element: node.local.clone(),
            gml_id: node.gml_id().map(str::to_owned),
            reason: format!("<{}> is not a supported CityModel property", node.local),
        });
        Ok(())
    }

    /// Warn when the document's `gml:Envelope` states that its coordinates
    /// are not three-dimensional.
    ///
    /// The dimension a *geometry* declares is read where the geometry is, and
    /// a ring in any dimension but three is skipped with a reason. This is
    /// the other place CityGML may state it, and it cannot be honoured the
    /// same way: the Envelope is one property of the document and the
    /// geometry is read from another, so by the time a coordinate is grouped
    /// the Envelope may not have been seen. Saying so is what can be done
    /// here, and it is better than a file whose every polygon is silently
    /// three-halves of itself.
    fn check_envelope_dimension(&mut self, envelope: &XmlNode) {
        let Some(dims) = envelope
            .attr(SRS_DIMENSION_ATTR)
            .and_then(|value| value.trim().parse::<usize>().ok())
            .filter(|dims| *dims != DIMS)
        else {
            return;
        };
        self.report.warnings.push(format!(
            "the document's <{ENVELOPE}> declares {SRS_DIMENSION_ATTR} {dims}: geometry that \
             does not state a dimension of its own is read as {DIMS}D"
        ));
    }

    /// Settle the document's CRS and hand back what the scan produced.
    fn finish(mut self) -> ScannedModel {
        let crs = self.resolve_crs();
        (self.objects, crs, self.appearances, self.report)
    }

    /// Reduce the `srsName` the document gave — the Envelope's for choice,
    /// else a geometry's — to a CityJSON reference system.
    ///
    /// Every way of ending up without one is a warning rather than an error:
    /// coordinates in an unnamed CRS are still coordinates, and the CityJSON
    /// `referenceSystem` is optional.
    fn resolve_crs(&mut self) -> Option<NormalizedCrs> {
        let Some(srs_name) = self
            .envelope_srs
            .take()
            .or_else(|| self.geometry_srs.take())
        else {
            self.report.warnings.push(NO_SRS_NAME.to_string());
            return None;
        };
        let Some(crs) = crs::normalize_srs(&srs_name) else {
            self.report.warnings.push(format!(
                "srsName {srs_name:?} is not a recognised EPSG reference; referenceSystem omitted"
            ));
            return None;
        };
        if crs::drops_vertical_component(&srs_name) {
            self.report.warnings.push(format!(
                "compound srsName {srs_name:?}: CityJSON holds one reference system, \
                 so the vertical component is dropped"
            ));
        }
        Some(crs)
    }
}

/// The `srsName` of the first GML element in a member that carries one.
///
/// CityGML puts the CRS on the document's Envelope, but a file without one
/// often still names it on each geometry, which is the only other place it
/// can be.
fn geometry_srs_name(member: &XmlNode) -> Option<String> {
    member
        .descendants()
        .filter(|node| node.ns == GML_NS)
        .find_map(|node| node.attr(SRS_NAME_ATTR))
        .map(str::to_owned)
}

/// Whether a node is the named element in one of those namespaces.
pub(crate) fn is_in(node: &XmlNode, namespaces: &[&str], local: &str) -> bool {
    node.local == local && namespaces.contains(&node.ns.as_str())
}

/// Advance to the first element and verify that it is a `CityModel`.
fn read_root<R: BufRead>(reader: &mut NsReader<R>, buf: &mut Vec<u8>) -> Result<(), CityGmlError> {
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

/// Read to the end of the input, reporting any malformed XML on the way.
fn drain_to_eof<R: BufRead>(
    reader: &mut NsReader<R>,
    buf: &mut Vec<u8>,
) -> Result<(), CityGmlError> {
    loop {
        buf.clear();
        match reader.read_event_into(buf) {
            Ok(Event::Eof) => return Ok(()),
            Ok(_) => {}
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
