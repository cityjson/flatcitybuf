//! An owned XML element tree, loaded one subtree at a time.
//!
//! CityGML is streamed, not loaded whole: the document reader walks to a
//! top-level city object and then hands *that element's subtree* to
//! [`load_subtree`], which materialises it as [`XmlNode`]. Everything above
//! that level stays streaming, so memory stays proportional to the largest
//! single city object rather than to the file.
//!
//! Nodes carry the *resolved* namespace URI, never the prefix: a document is
//! free to bind `gml:` to anything, or to make GML the default namespace, and
//! the readers must not care.
//!
//! Attributes are resolved too, and it matters more than it looks. `gml:id`
//! is what an `xlink:href` names and what an `app:target` paints, so an
//! element that happens to carry an *unqualified* `id` — an application
//! schema is free to define one — must not answer to [`XmlNode::gml_id`], or
//! a reference would resolve to the wrong element. An attribute with no
//! prefix is in no namespace (XML Namespaces § 6.2, unlike an element, which
//! takes the default), so the two are kept apart by storing a qualified
//! attribute under `{namespace}|{local name}` and an unqualified one under
//! its local name alone.

use std::io::BufRead;

use quick_xml::errors::IllFormedError;
use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesRef, BytesStart, Event};
use quick_xml::name::{QName, ResolveResult};
use quick_xml::NsReader;
use quick_xml::XmlVersion;

use crate::gml::GML_NS;
use crate::{xml_error, CityGmlError};

/// One XML element, with its namespace resolved and its subtree owned.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct XmlNode {
    /// Resolved namespace URI, or the empty string when the element is in no
    /// namespace.
    pub ns: String,
    /// Local element name, with any prefix stripped.
    pub local: String,
    /// Attributes as (key, value), where the key is the local name for an
    /// attribute in no namespace and `{namespace}|{local name}` for one in a
    /// namespace. Namespace declarations are dropped. Reach them through
    /// [`XmlNode::attr`] and [`XmlNode::attr_ns`] rather than by hand.
    pub attrs: Vec<(String, String)>,
    /// Direct text content, concatenated across text and CDATA runs and
    /// trimmed. Text belonging to child elements is not included.
    pub text: String,
    /// Child elements, in document order.
    pub children: Vec<XmlNode>,
}

impl XmlNode {
    /// The value of the attribute in *no namespace* with this local name.
    ///
    /// This is the one to reach an ordinary XML attribute by — `srsName`,
    /// `orientation`, `uri` — which is written without a prefix and so is in
    /// no namespace. A qualified attribute is reached by [`attr_ns`] instead,
    /// and never answers here.
    ///
    /// [`attr_ns`]: XmlNode::attr_ns
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.value_of(name)
    }

    /// The value of the attribute in `ns` with this local name.
    pub(crate) fn attr_ns(&self, ns: &str, local: &str) -> Option<&str> {
        self.value_of(&attribute_key(ns, local))
    }

    fn value_of(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    /// This node and all its descendants, depth-first in document order.
    pub fn descendants(&self) -> impl Iterator<Item = &XmlNode> {
        Descendants { stack: vec![self] }
    }

    /// The `gml:id` of this element, if it has one.
    ///
    /// Only a properly qualified `gml:id` counts: this id is what an
    /// `xlink:href` and an `app:target` name, and an `id` an application
    /// schema defined means something else entirely.
    pub fn gml_id(&self) -> Option<&str> {
        self.attr_ns(GML_NS, GML_ID_ATTR)
    }

    /// The `xlink:href` of this element, if it has one.
    ///
    /// As with [`gml_id`](Self::gml_id), only the qualified attribute counts:
    /// an unqualified `href` is not an XLink locator, and following one would
    /// be following whatever a foreign schema meant by the word.
    pub(crate) fn href(&self) -> Option<&str> {
        self.attr_ns(XLINK_NS, HREF_ATTR)
    }
}

/// Local name of the GML identifier attribute, which is only ever read in
/// [`GML_NS`].
const GML_ID_ATTR: &str = "id";

/// The XLink namespace, and the local name of the locator attribute CityGML
/// shares geometry by.
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";
const HREF_ATTR: &str = "href";

/// What separates an attribute's namespace from its local name in the key it
/// is stored under. A local name cannot hold one, so the split is
/// unambiguous, and an attribute in no namespace has no separator at all.
const NS_SEPARATOR: char = '|';

/// The key a qualified attribute is stored and looked up under.
fn attribute_key(ns: &str, local: &str) -> String {
    format!("{ns}{NS_SEPARATOR}{local}")
}

/// Prefix marking a namespace declaration rather than a real attribute.
const XMLNS: &[u8] = b"xmlns";

/// Depth-first iterator over a node and its descendants.
struct Descendants<'a> {
    /// Nodes still to visit, deepest-last so that `pop` yields document order.
    stack: Vec<&'a XmlNode>,
}

impl<'a> Iterator for Descendants<'a> {
    type Item = &'a XmlNode;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.stack.pop()?;
        self.stack.extend(node.children.iter().rev());
        Some(node)
    }
}

/// Read the element whose start tag was just consumed, up to its matching end
/// tag, into an owned [`XmlNode`].
///
/// The reader is left positioned immediately after that end tag, so a caller
/// can keep streaming siblings. `start` must come from an [`Event::Start`]:
/// an [`Event::Empty`] has no end tag, and passing one would swallow the
/// following siblings instead.
///
/// # Errors
///
/// Returns [`CityGmlError::Xml`] for malformed XML, including a subtree that
/// the input ends in the middle of.
pub fn load_subtree<R: BufRead>(
    reader: &mut NsReader<R>,
    start: BytesStart,
) -> Result<XmlNode, CityGmlError> {
    // Elements currently open, outermost first; the last is the one that text
    // and child elements belong to.
    let mut open = vec![node_from_start(reader, &start)?];
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let node = node_from_start(reader, &e)?;
                open.push(node);
            }
            Ok(Event::Empty(e)) => {
                let node = node_from_start(reader, &e)?;
                push_child(&mut open, node);
            }
            Ok(Event::End(e)) => {
                // quick-xml checks that the name matches the open element, so
                // an unbalanced document has already failed by this point.
                let Some(mut node) = open.pop() else {
                    return Err(ill_formed(
                        reader,
                        IllFormedError::UnmatchedEndTag(name_of(e.name())),
                    ));
                };
                trim_in_place(&mut node.text);
                if open.is_empty() {
                    return Ok(node);
                }
                push_child(&mut open, node);
            }
            Ok(Event::Text(e)) => {
                let text = e
                    .xml10_content()
                    .map_err(|source| xml_error(reader, source.into()))?;
                push_text(&mut open, &text);
            }
            Ok(Event::CData(e)) => {
                let text = e
                    .xml10_content()
                    .map_err(|source| xml_error(reader, source.into()))?;
                push_text(&mut open, &text);
            }
            Ok(Event::GeneralRef(e)) => {
                let replacement = resolve_reference(reader, &e)?;
                push_text(&mut open, &replacement);
            }
            Ok(Event::Eof) => return Err(unexpected_eof(reader, &start)),
            // Comments, processing instructions, declarations and doctypes
            // carry nothing this converter needs.
            Ok(_) => {}
            Err(source) => return Err(xml_error(reader, source)),
        }
    }
}

/// Build a childless [`XmlNode`] from a self-closing element's tag.
///
/// [`load_subtree`] cannot read one: an [`Event::Empty`] has no end tag, so
/// waiting for one would swallow the following siblings. Callers that walk a
/// level of the document therefore handle `Empty` with this and `Start` with
/// [`load_subtree`], and treat the two results alike.
///
/// # Errors
///
/// Returns [`CityGmlError::Xml`] when an attribute cannot be decoded.
pub(crate) fn leaf_node<R>(
    reader: &NsReader<R>,
    start: &BytesStart,
) -> Result<XmlNode, CityGmlError> {
    node_from_start(reader, start)
}

/// Parse a whole XML string into the [`XmlNode`] of its root element.
///
/// This exists so that the geometry readers can be tested against XML
/// literals; the document reader streams instead, and calls [`load_subtree`]
/// directly.
#[cfg(test)]
pub(crate) fn parse_str_for_tests(xml: &str) -> Result<XmlNode, CityGmlError> {
    // `&[u8]` is a `BufRead`, which `NsReader::from_str`'s reader is not.
    let mut reader = NsReader::from_reader(xml.as_bytes());
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let start = e.into_owned();
                return load_subtree(&mut reader, start);
            }
            Ok(Event::Empty(e)) => return node_from_start(&reader, &e),
            Ok(Event::Eof) => {
                return Err(CityGmlError::UnsupportedRoot(
                    crate::NO_ROOT_ELEMENT.to_string(),
                ))
            }
            Ok(_) => {}
            Err(source) => return Err(xml_error(&reader, source)),
        }
    }
}

/// Build a childless node from a start (or empty) tag, resolving its
/// namespace and decoding its attributes.
fn node_from_start<R>(reader: &NsReader<R>, start: &BytesStart) -> Result<XmlNode, CityGmlError> {
    let (ns, local) = reader.resolver().resolve_element(start.name());
    let ns = match ns {
        ResolveResult::Bound(ns) => String::from_utf8_lossy(ns.as_ref()).into_owned(),
        // An unbound prefix is a namespace error, which quick-xml surfaces
        // through the event itself; treat the rest as "no namespace".
        _ => String::new(),
    };

    let mut attrs = Vec::new();
    for attr in start.attributes() {
        let attr = attr.map_err(|source| xml_error(reader, source.into()))?;
        let name = attr.key;
        if name.as_ref() == XMLNS || name.prefix().is_some_and(|p| p.as_ref() == XMLNS) {
            continue;
        }
        let value = attr
            .decoded_and_normalized_value(XmlVersion::Implicit1_0, reader.decoder())
            .map_err(|source| xml_error(reader, source))?;
        let (ns, local) = reader.resolver().resolve_attribute(name);
        let local = String::from_utf8_lossy(local.as_ref()).into_owned();
        // An unbound prefix is a namespace error, which quick-xml surfaces
        // through the event itself; an attribute with no prefix is in no
        // namespace by definition, and both are stored under the bare local
        // name, where only an unqualified lookup can reach them.
        let key = match ns {
            ResolveResult::Bound(ns) => {
                attribute_key(&String::from_utf8_lossy(ns.as_ref()), &local)
            }
            _ => local,
        };
        attrs.push((key, value.into_owned()));
    }

    Ok(XmlNode {
        ns,
        local: String::from_utf8_lossy(local.as_ref()).into_owned(),
        attrs,
        text: String::new(),
        children: Vec::new(),
    })
}

/// Attach a finished node to the element that is currently open.
fn push_child(open: &mut [XmlNode], node: XmlNode) {
    if let Some(parent) = open.last_mut() {
        parent.children.push(node);
    }
}

/// Append a run of character data to the element that is currently open.
///
/// Runs are concatenated raw and trimmed once the element closes, so that
/// text split by an entity reference or a CDATA section rejoins without a
/// spurious gap, while pretty-printing whitespace still disappears.
fn push_text(open: &mut [XmlNode], text: &str) {
    if let Some(node) = open.last_mut() {
        node.text.push_str(text);
    }
}

/// Resolve a general entity reference to its replacement text.
///
/// Character references and the five predefined entities are replaced;
/// anything else would have to come from a document type definition, which
/// this converter does not read, so it contributes nothing.
fn resolve_reference<R>(
    reader: &NsReader<R>,
    reference: &BytesRef,
) -> Result<String, CityGmlError> {
    if let Some(c) = reference
        .resolve_char_ref()
        .map_err(|source| xml_error(reader, source))?
    {
        return Ok(c.to_string());
    }
    let name = reference
        .decode()
        .map_err(|source| xml_error(reader, source.into()))?;
    Ok(resolve_predefined_entity(&name).unwrap_or("").to_string())
}

/// Strip an element's accumulated text of surrounding whitespace, in place.
///
/// A `posList` can hold tens of thousands of coordinates, and reallocating
/// that string just to drop the layout whitespace around it is a copy this
/// converter does once per element.
fn trim_in_place(text: &mut String) {
    text.truncate(text.trim_end().len());
    let leading = text.len() - text.trim_start().len();
    text.drain(..leading);
}

/// The error for input that ends in the middle of a subtree.
fn unexpected_eof<R>(reader: &NsReader<R>, start: &BytesStart) -> CityGmlError {
    ill_formed(reader, IllFormedError::MissingEndTag(name_of(start.name())))
}

/// Attach the reader's position to an ill-formed-document error.
fn ill_formed<R>(reader: &NsReader<R>, error: IllFormedError) -> CityGmlError {
    xml_error(reader, error.into())
}

/// An element name as a lossy string, for error messages.
fn name_of(name: QName) -> String {
    String::from_utf8_lossy(name.as_ref()).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn node(xml: &str) -> XmlNode {
        parse_str_for_tests(xml).unwrap()
    }

    #[test]
    fn root_carries_resolved_namespace_and_local_name() {
        let n = node(r#"<gml:Polygon xmlns:gml="http://www.opengis.net/gml"/>"#);
        assert_eq!(n.ns, "http://www.opengis.net/gml");
        assert_eq!(n.local, "Polygon");

        // The same element, with GML as the default namespace and no prefix.
        let n = node(r#"<Polygon xmlns="http://www.opengis.net/gml"/>"#);
        assert_eq!(n.ns, "http://www.opengis.net/gml");
        assert_eq!(n.local, "Polygon");

        // ... and with no namespace at all.
        let n = node(r#"<Polygon/>"#);
        assert_eq!(n.ns, "");
        assert_eq!(n.local, "Polygon");
    }

    #[test]
    fn attributes_are_reached_by_namespace_and_local_name() {
        let n = node(
            r##"<Building xmlns:gml="http://www.opengis.net/gml"
                          xmlns:xlink="http://www.w3.org/1999/xlink"
                          gml:id="b1" xlink:href="#surface-7" plain="yes"/>"##,
        );
        assert_eq!(n.gml_id(), Some("b1"));
        assert_eq!(n.href(), Some("#surface-7"));
        // An unprefixed attribute is in no namespace, which is what `attr`
        // reads; the qualified ones are not reachable that way.
        assert_eq!(n.attr("plain"), Some("yes"));
        assert_eq!(n.attr("id"), None);
        assert_eq!(n.attr("href"), None);
        assert_eq!(n.attr("missing"), None);
        // Namespace declarations are not attributes of the element.
        assert_eq!(n.attr("gml"), None);
        assert_eq!(n.attr("xmlns"), None);
        assert_eq!(n.attrs.len(), 3);
    }

    /// An unqualified `id` is not a `gml:id`. An application schema is free
    /// to define one, and treating it as the GML identifier would let an
    /// `xlink:href` or an `app:target` resolve to an element the document
    /// never named.
    #[test]
    fn an_unqualified_id_is_not_a_gml_id() {
        let n = node(r#"<Building xmlns:gml="http://www.opengis.net/gml" id="b1"/>"#);
        assert_eq!(n.gml_id(), None);
        assert_eq!(n.attr("id"), Some("b1"));
    }

    /// Nor is an `id` an ADE put in a namespace of its own — and the two do
    /// not shadow the real one when a document carries all three.
    #[test]
    fn an_id_in_a_foreign_namespace_is_not_a_gml_id() {
        let n = node(
            r#"<Building xmlns:gml="http://www.opengis.net/gml"
                         xmlns:ade="urn:example:ade"
                         ade:id="ade-1" id="plain" gml:id="b1"/>"#,
        );
        assert_eq!(n.gml_id(), Some("b1"));
        assert_eq!(n.attr("id"), Some("plain"));
        assert_eq!(n.attr_ns("urn:example:ade", "id"), Some("ade-1"));
    }

    /// The same rule for the locator: only `xlink:href` is one.
    #[test]
    fn only_an_xlink_href_is_a_locator() {
        let n = node(
            r##"<surfaceMember xmlns:ade="urn:example:ade"
                               href="#unqualified" ade:href="#foreign"/>"##,
        );
        assert_eq!(n.href(), None);
        assert_eq!(n.attr("href"), Some("#unqualified"));

        // A prefix bound to the XLink namespace is one whatever it is called.
        let n = node(r##"<surfaceMember xmlns:x="http://www.w3.org/1999/xlink" x:href="#p1"/>"##);
        assert_eq!(n.href(), Some("#p1"));
    }

    #[test]
    fn attribute_values_are_unescaped() {
        let n = node(r#"<a v="1 &lt; 2 &amp; 3 &gt; 2"/>"#);
        assert_eq!(n.attr("v"), Some("1 < 2 & 3 > 2"));
    }

    #[test]
    fn text_is_concatenated_and_trimmed() {
        // Text split by an entity reference and by a CDATA section still
        // reads back as one string, and the surrounding layout whitespace is
        // trimmed away.
        let n = node("<a>\n  one &amp; <![CDATA[two]]> three\n</a>");
        assert_eq!(n.text, "one & two three");
    }

    #[test]
    fn text_of_children_does_not_leak_into_the_parent() {
        let n = node("<a>outer<b>inner</b></a>");
        assert_eq!(n.text, "outer");
        assert_eq!(n.children[0].text, "inner");

        // A parent holding only layout whitespace has empty text.
        let n = node("<a>\n  <b>inner</b>\n</a>");
        assert_eq!(n.text, "");
    }

    #[test]
    fn nested_same_name_elements_are_depth_balanced() {
        let n = node("<a><a><a>deep</a></a><a>sibling</a></a>");
        assert_eq!(n.children.len(), 2);
        assert_eq!(n.children[0].children[0].text, "deep");
        assert_eq!(n.children[1].text, "sibling");
    }

    #[test]
    fn empty_elements_become_childless_nodes() {
        let n = node(r#"<a><b x="1"/><c/></a>"#);
        assert_eq!(n.children.len(), 2);
        assert_eq!(n.children[0].local, "b");
        assert_eq!(n.children[0].attr("x"), Some("1"));
        assert!(n.children[0].children.is_empty());
        assert_eq!(n.children[1].local, "c");
    }

    #[test]
    fn descendants_are_depth_first_and_include_self() {
        let n = node("<a><b><c/></b><d/></a>");
        let names: Vec<&str> = n.descendants().map(|x| x.local.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn load_subtree_stops_at_the_matching_end_tag() {
        let xml = "<root><first><x/></first><second/></root>";
        let mut reader = NsReader::from_reader(xml.as_bytes());
        let mut buf = Vec::new();
        // Skip past <root> and stop on <first>.
        let start = loop {
            buf.clear();
            match reader.read_event_into(&mut buf).unwrap() {
                Event::Start(e) if e.local_name().as_ref() == b"first" => break e.into_owned(),
                Event::Eof => panic!("no <first> element"),
                _ => {}
            }
        };
        let first = load_subtree(&mut reader, start).unwrap();
        assert_eq!(first.local, "first");
        assert_eq!(first.children.len(), 1);

        // The reader is left positioned right after </first>.
        buf.clear();
        match reader.read_event_into(&mut buf).unwrap() {
            Event::Empty(e) => assert_eq!(e.local_name().as_ref(), b"second"),
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn truncated_subtree_is_an_xml_error_not_a_panic() {
        let err = parse_str_for_tests("<a><b>text</b>").unwrap_err();
        assert!(matches!(err, CityGmlError::Xml { .. }), "{err:?}");
    }

    #[test]
    fn mismatched_end_tag_is_an_xml_error() {
        let err = parse_str_for_tests("<a><b></c></a>").unwrap_err();
        assert!(matches!(err, CityGmlError::Xml { .. }), "{err:?}");
    }

    #[test]
    fn document_without_an_element_is_rejected() {
        let err = parse_str_for_tests("<!-- nothing here -->").unwrap_err();
        assert!(matches!(err, CityGmlError::UnsupportedRoot(_)), "{err:?}");
    }

    #[test]
    fn comments_and_processing_instructions_are_ignored() {
        let n = node("<a><!-- c --><?pi data?><b/></a>");
        assert_eq!(n.children.len(), 1);
        assert_eq!(n.children[0].local, "b");
        assert_eq!(n.text, "");
    }
}
