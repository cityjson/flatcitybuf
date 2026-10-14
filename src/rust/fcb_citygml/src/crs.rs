//! Normalisation of GML `srsName` values into the CityJSON
//! `metadata.referenceSystem` form, plus the axis-order question that comes
//! with it.
//!
//! CityGML documents in the wild name the same CRS in at least four ways:
//!
//! | Form | Example |
//! |---|---|
//! | legacy short code | `EPSG:25832` |
//! | OGC URN | `urn:ogc:def:crs:EPSG::25832` |
//! | compound OGC URN | `urn:ogc:def:crs,crs:EPSG::25832,crs:EPSG::5783` |
//! | OGC URL | `http://www.opengis.net/def/crs/EPSG/0/25832` |
//!
//! CityJSON wants exactly one of them — the OGC URL — so every accepted form
//! is reduced to its EPSG code and re-emitted as
//! `https://www.opengis.net/def/crs/EPSG/0/{code}`.
//!
//! The forms also disagree about axis order. `EPSG:4326` is understood by
//! convention as x = longitude, while the URN and OGC-URL forms carry the
//! CRS's *authoritative* axis order, which for most geographic CRSs is
//! latitude first. Coordinates from those forms therefore have to be swapped
//! to reach CityJSON's x = easting/longitude ordering; see
//! [`NormalizedCrs::swap_axes`].

/// Prefix of the OGC URL form, with the scheme already stripped.
const OGC_URL_SUFFIX: &str = "://www.opengis.net/def/crs/";

/// Prefixes of the OGC URN form. `urn:x-ogc` is the pre-2007 spelling and is
/// still produced by some exporters.
const URN_PREFIXES: [&str; 2] = ["urn:ogc:def:crs", "urn:x-ogc:def:crs"];

/// The authority this module understands, lowercased.
const EPSG_AUTHORITY: &str = "epsg";

/// EPSG codes of geographic CRSs whose authoritative axis order is
/// latitude, longitude — i.e. the ones that need swapping when named by a
/// URN or OGC URL.
///
/// This is a pragmatic list covering the datums that actually appear in
/// CityGML data, not the full EPSG registry: resolving axis order properly
/// would mean depending on a CRS database. Extend it as datasets demand.
const LAT_LON_EPSG_CODES: [u32; 6] = [
    4326, // WGS 84
    4258, // ETRS89
    4269, // NAD83
    4283, // GDA94
    4171, // RGF93 v1
    4617, // NAD83(CSRS)
];

/// A `srsName` reduced to the CityJSON reference-system URL, together with
/// whether the source named its axes latitude-first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedCrs {
    /// OGC URL form, e.g. `https://www.opengis.net/def/crs/EPSG/0/25832`.
    pub reference_system: String,
    /// True when the source coordinates are ordered latitude, longitude and
    /// must be swapped to CityJSON's x, y ordering.
    pub swap_axes: bool,
}

/// Which `srsName` spelling a code was extracted from, since the spelling —
/// not the code — decides whose axis-order convention applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SrsForm {
    /// `EPSG:4326`: x = longitude by convention, whatever EPSG says.
    Legacy,
    /// URN or OGC URL: the CRS's authoritative axis order applies.
    Authoritative,
}

/// Normalise a GML `srsName` into a CityJSON reference system.
///
/// Returns `None` for anything that is not a recognisable EPSG reference —
/// an unsupported authority, a malformed URN, or a non-numeric code — so
/// that callers can report the value rather than guess at it.
///
/// # Examples
///
/// ```
/// use fcb_citygml::crs::normalize_srs;
///
/// let crs = normalize_srs("urn:ogc:def:crs:EPSG::4326").unwrap();
/// assert_eq!(crs.reference_system, "https://www.opengis.net/def/crs/EPSG/0/4326");
/// assert!(crs.swap_axes);
///
/// assert!(normalize_srs("urn:ogc:def:crs:OGC:1.3:CRS84").is_none());
/// ```
pub fn normalize_srs(srs_name: &str) -> Option<NormalizedCrs> {
    let (code, form) = parse_epsg_code(srs_name.trim())?;
    Some(NormalizedCrs {
        reference_system: format!("https://www.opengis.net/def/crs/EPSG/0/{code}"),
        swap_axes: form == SrsForm::Authoritative && LAT_LON_EPSG_CODES.contains(&code),
    })
}

/// Whether a `srsName` names more than one CRS, so that [`normalize_srs`]
/// keeps only its horizontal component.
///
/// Only the compound URN form can carry a vertical component, and it always
/// says so with a comma; nothing else here has to be parsed to tell. The
/// caller warns, because dropping the vertical CRS silently would leave a
/// document claiming a 2D reference system for 3D coordinates.
pub(crate) fn drops_vertical_component(srs_name: &str) -> bool {
    let srs_name = srs_name.trim().to_ascii_lowercase();
    let Some(body) = URN_PREFIXES
        .iter()
        .find_map(|prefix| srs_name.strip_prefix(prefix))
    else {
        return false;
    };
    let Some(components) = body.strip_prefix(',') else {
        return false;
    };
    components.split(',').filter(|c| !c.is_empty()).count() > 1
}

/// Extract the EPSG code and the spelling it came from.
fn parse_epsg_code(srs_name: &str) -> Option<(u32, SrsForm)> {
    // Only the code's digits survive, so case folding the whole value is
    // safe and spares every comparison below an `eq_ignore_ascii_case`.
    let srs_name = srs_name.to_ascii_lowercase();

    if let Some(rest) = strip_ogc_url_prefix(&srs_name) {
        return parse_ogc_url_path(rest).map(|code| (code, SrsForm::Authoritative));
    }
    if let Some(rest) = URN_PREFIXES
        .iter()
        .find_map(|prefix| srs_name.strip_prefix(prefix))
    {
        return parse_urn_body(rest).map(|code| (code, SrsForm::Authoritative));
    }
    parse_legacy(&srs_name).map(|code| (code, SrsForm::Legacy))
}

/// Strip `http://www.opengis.net/def/crs/` or its `https` spelling.
fn strip_ogc_url_prefix(srs_name: &str) -> Option<&str> {
    let rest = srs_name
        .strip_prefix("http")
        .map(|rest| rest.strip_prefix('s').unwrap_or(rest))?;
    rest.strip_prefix(OGC_URL_SUFFIX)
}

/// Parse the `{authority}/{version}/{code}` tail of an OGC URL.
fn parse_ogc_url_path(path: &str) -> Option<u32> {
    let mut segments = path.trim_end_matches('/').split('/');
    let authority = segments.next()?;
    // The version segment is free-form ("0", "9.9.1") and carries nothing
    // this converter needs.
    let _version = segments.next()?;
    let code = segments.next()?;
    if segments.next().is_some() || authority != EPSG_AUTHORITY {
        return None;
    }
    parse_code(code)
}

/// Parse whatever follows `urn:ogc:def:crs`, simple or compound.
///
/// A compound URN — `urn:ogc:def:crs,crs:EPSG::25832,crs:EPSG::5783` — pairs
/// a horizontal CRS with a vertical one. CityJSON's `referenceSystem` holds a
/// single CRS, so the horizontal component (always the first) is the one
/// kept, and the vertical component is dropped.
fn parse_urn_body(body: &str) -> Option<u32> {
    if let Some(components) = body.strip_prefix(',') {
        let horizontal = components.split(',').next()?;
        return parse_urn_component(horizontal.strip_prefix("crs")?);
    }
    parse_urn_component(body)
}

/// Parse `:{authority}:{version}:{code}`, the tail shared by a simple URN and
/// by each component of a compound one. The version is commonly empty
/// (`EPSG::25832`) but may be a revision (`EPSG:6.12:25832`).
fn parse_urn_component(component: &str) -> Option<u32> {
    let mut fields = component.strip_prefix(':')?.split(':');
    let authority = fields.next()?;
    let _version = fields.next()?;
    let code = fields.next()?;
    if fields.next().is_some() || authority != EPSG_AUTHORITY {
        return None;
    }
    parse_code(code)
}

/// Parse the legacy `EPSG:{code}` form, tolerating the `EPSG::{code}`
/// spelling that some exporters emit.
fn parse_legacy(srs_name: &str) -> Option<u32> {
    let code = srs_name.strip_prefix(EPSG_AUTHORITY)?.strip_prefix(':')?;
    parse_code(code.strip_prefix(':').unwrap_or(code))
}

/// Parse an EPSG code, rejecting anything that is not plain ASCII digits —
/// `u32::from_str` alone would accept a leading `+`.
fn parse_code(code: &str) -> Option<u32> {
    if code.is_empty() || !code.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    code.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn epsg_forms_normalize_to_ogc_url() {
        for form in [
            "EPSG:25832",
            "urn:ogc:def:crs:EPSG::25832",
            "http://www.opengis.net/def/crs/EPSG/0/25832",
            "https://www.opengis.net/def/crs/EPSG/0/25832",
        ] {
            let c = normalize_srs(form).unwrap();
            assert_eq!(
                c.reference_system,
                "https://www.opengis.net/def/crs/EPSG/0/25832"
            );
            assert!(!c.swap_axes);
        }
    }
    #[test]
    fn compound_urn_takes_horizontal_component() {
        let c = normalize_srs("urn:ogc:def:crs,crs:EPSG::25832,crs:EPSG::5783").unwrap();
        assert_eq!(
            c.reference_system,
            "https://www.opengis.net/def/crs/EPSG/0/25832"
        );
    }
    #[test]
    fn urn_4326_swaps_axes() {
        let c = normalize_srs("urn:ogc:def:crs:EPSG::4326").unwrap();
        assert_eq!(
            c.reference_system,
            "https://www.opengis.net/def/crs/EPSG/0/4326"
        );
        assert!(c.swap_axes);
    }
    #[test]
    fn legacy_epsg_colon_4326_does_not_swap() {
        // "EPSG:4326" is the legacy x=lon convention; only the urn/OGC-URL forms are lat/lon.
        assert!(!normalize_srs("EPSG:4326").unwrap().swap_axes);
    }
    #[test]
    fn only_a_multi_component_urn_drops_a_vertical_crs() {
        assert!(drops_vertical_component(
            "urn:ogc:def:crs,crs:EPSG::25832,crs:EPSG::5783"
        ));
        assert!(drops_vertical_component(
            "  URN:X-OGC:DEF:CRS,crs:EPSG::25832,crs:EPSG::5783  "
        ));
        for form in [
            "urn:ogc:def:crs:EPSG::25832",
            "urn:ogc:def:crs,crs:EPSG::25832", // compound spelling, one CRS
            "EPSG:25832",
            "https://www.opengis.net/def/crs/EPSG/0/25832",
            "",
        ] {
            assert!(!drops_vertical_component(form), "{form}");
        }
    }

    #[test]
    fn unknown_is_none() {
        assert!(normalize_srs("CRS:84unknown-junk").is_none());
    }

    #[test]
    fn versioned_urn_and_url_keep_the_code() {
        for form in [
            "urn:ogc:def:crs:EPSG:6.12:25832",
            "urn:x-ogc:def:crs:EPSG:6.12:25832",
            "https://www.opengis.net/def/crs/EPSG/9.9.1/25832",
        ] {
            assert_eq!(
                normalize_srs(form).unwrap().reference_system,
                "https://www.opengis.net/def/crs/EPSG/0/25832"
            );
        }
    }

    #[test]
    fn authority_case_and_surrounding_space_are_ignored() {
        let c = normalize_srs("  urn:ogc:def:crs:epsg::4258  ").unwrap();
        assert_eq!(
            c.reference_system,
            "https://www.opengis.net/def/crs/EPSG/0/4258"
        );
        assert!(c.swap_axes);
    }

    #[test]
    fn non_epsg_authorities_are_none() {
        for form in [
            "urn:ogc:def:crs:OGC:1.3:CRS84",
            "http://www.opengis.net/def/crs/OGC/1.3/CRS84",
            "",
        ] {
            assert!(normalize_srs(form).is_none(), "{form} should be rejected");
        }
    }

    #[test]
    fn malformed_epsg_references_are_none() {
        for form in [
            "EPSG:",                                      // no code
            "EPSG:+25832",                                // signed code
            "EPSG:25832m",                                // trailing junk
            "urn:ogc:def:crs:EPSG:25832",                 // version field missing
            "urn:ogc:def:crs:EPSG::25832:extra",          // one field too many
            "https://www.opengis.net/def/crs/EPSG/25832", // version missing
        ] {
            assert!(normalize_srs(form).is_none(), "{form} should be rejected");
        }
    }
}
