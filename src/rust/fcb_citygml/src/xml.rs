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

use std::io::BufRead;

use quick_xml::errors::IllFormedError;
use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesRef, BytesStart, Event};
use quick_xml::name::{QName, ResolveResult};
use quick_xml::NsReader;
use quick_xml::XmlVersion;

use crate::{xml_error, CityGmlError};

/// One XML element, with its namespace resolved and its subtree owned.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct XmlNode {
    /// Resolved namespace URI, or the empty string when the element is in no
    /// namespace.
    pub ns: String,
    /// Local element name, with any prefix stripped.
    pub local: String,
    /// Attributes as (local name, value); namespace declarations are dropped.
    pub attrs: Vec<(String, String)>,
    /// Direct text content, concatenated across text and CDATA runs and
    /// trimmed. Text belonging to child elements is not included.
    pub text: String,
    /// Child elements, in document order.
    pub children: Vec<XmlNode>,
}

impl XmlNode {
    /// The value of the attribute with this local name, if present.
    ///
    /// Attributes are matched on their local name alone, so `gml:id` is
    /// reached as `attr("id")` and `xlink:href` as `attr("href")`.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// The first direct child element with this local name.
    pub fn child(&self, local: &str) -> Option<&XmlNode> {
        self.children.iter().find(|child| child.local == local)
    }

    /// Every direct child with this local name, in document order.
    pub fn children_named<'a>(&'a self, local: &'a str) -> impl Iterator<Item = &'a XmlNode> {
        self.children
            .iter()
            .filter(move |child| child.local == local)
    }

    /// This node and all its descendants, depth-first in document order.
    pub fn descendants(&self) -> impl Iterator<Item = &XmlNode> {
        Descendants { stack: vec![self] }
    }

    /// The `gml:id` of this element, if it has one.
    pub fn gml_id(&self) -> Option<&str> {
        self.attr(GML_ID_ATTR)
    }
}

/// Local name of the GML identifier attribute.
const GML_ID_ATTR: &str = "id";

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
        attrs.push((
            String::from_utf8_lossy(name.local_name().as_ref()).into_owned(),
            value.into_owned(),
        ));
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
    fn child_and_children_named_match_across_prefixes() {
        let n = node(
            r#"<root xmlns:a="urn:x" xmlns:b="urn:y">
                 <a:item>1</a:item>
                 <b:item>2</b:item>
                 <a:other>3</a:other>
               </root>"#,
        );
        assert_eq!(n.child("item").unwrap().text, "1");
        assert_eq!(n.child("other").unwrap().text, "3");
        assert!(n.child("missing").is_none());

        let items: Vec<&str> = n.children_named("item").map(|c| c.text.as_str()).collect();
        assert_eq!(items, vec!["1", "2"]);
        let namespaces: Vec<&str> = n.children_named("item").map(|c| c.ns.as_str()).collect();
        assert_eq!(namespaces, vec!["urn:x", "urn:y"]);
        assert_eq!(n.children_named("missing").count(), 0);
    }

    #[test]
    fn attributes_are_stored_by_local_name() {
        let n = node(
            r##"<Building xmlns:gml="http://www.opengis.net/gml"
                          xmlns:xlink="http://www.w3.org/1999/xlink"
                          gml:id="b1" xlink:href="#surface-7" plain="yes"/>"##,
        );
        assert_eq!(n.attr("id"), Some("b1"));
        assert_eq!(n.gml_id(), Some("b1"));
        assert_eq!(n.attr("href"), Some("#surface-7"));
        assert_eq!(n.attr("plain"), Some("yes"));
        assert_eq!(n.attr("missing"), None);
        // Namespace declarations are not attributes of the element.
        assert_eq!(n.attr("gml"), None);
        assert_eq!(n.attr("xmlns"), None);
        assert_eq!(n.attrs.len(), 3);
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
        assert_eq!(n.child("b").unwrap().text, "inner");

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
