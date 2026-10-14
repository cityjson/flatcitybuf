/// Errors produced while converting a CityGML document to CityJSON.
///
/// The policy is: malformed structure is a hard error, while valid but
/// unsupported content is reported through
/// [`ParseReport`](crate::ParseReport) instead of failing the conversion.
#[derive(Debug, thiserror::Error)]
pub enum CityGmlError {
    #[error("XML error at byte {position}: {source}")]
    Xml {
        position: u64,
        #[source]
        source: quick_xml::Error,
    },
    #[error("root element is <{0}>, expected CityModel")]
    UnsupportedRoot(String),
    #[error("unresolvable xlink href {href} in {context}")]
    UnresolvableXlink { href: String, context: String },
    #[error("invalid geometry in {context}: {reason}")]
    InvalidGeometry { context: String, reason: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
