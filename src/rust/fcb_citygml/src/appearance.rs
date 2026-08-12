//! Appearance: the CityGML appearance module's surface data, as the material
//! and texture palettes CityJSON writes.
//!
//! CityGML states appearance *away from* the geometry. An `app:Appearance`
//! holds a theme and a list of surface data — an `app:X3DMaterial`, an
//! `app:ParameterizedTexture` — and each of those names the polygons it
//! applies to by `gml:id`, through one `app:target` per polygon. CityJSON
//! states it the other way round: a document-level palette, and one index per
//! surface — per *ring*, for a texture — written on the geometry that owns it.
//!
//! So this module does the reading half only — surface data in, one
//! [`SurfaceData`] apiece out, targets kept as the polygon and ring ids they
//! name — and [`crate::convert`] does the join, which is the half that needs
//! the geometry.
//!
//! An `app:Appearance` reaches here from either of the two places CityGML
//! allows it: the `app:appearanceMember` property of the `CityModel`, and the
//! `app:appearance` property of a city object. [`parse_appearances`] takes
//! whatever node each was found as and looks for `app:Appearance` among its
//! descendants, so a caller need not know which of the two it holds.

use cjseq::{Color, MaterialObject, TextureFormat, TextureObject};
use serde_json::Value;

use crate::gml::is_gml;
use crate::xml::XmlNode;
use crate::{is_in, ParseReport, Skipped, APPEARANCE_NS};

/// Local names of the appearance elements this module reads. As everywhere in
/// this crate, a local name is only ever matched together with a namespace.
const APPEARANCE: &str = "Appearance";
const THEME: &str = "theme";
const SURFACE_DATA_MEMBER: &str = "surfaceDataMember";
const X3D_MATERIAL: &str = "X3DMaterial";
const PARAMETERIZED_TEXTURE: &str = "ParameterizedTexture";
const TARGET: &str = "target";
const TEX_COORD_LIST: &str = "TexCoordList";
const TEXTURE_COORDINATES: &str = "textureCoordinates";
const TEX_COORD_GEN: &str = "TexCoordGen";

/// Local names of the properties of an `app:ParameterizedTexture` that
/// CityJSON's `Texture` object keeps.
const IMAGE_URI: &str = "imageURI";
const MIME_TYPE: &str = "mimeType";
const WRAP_MODE: &str = "wrapMode";
const TEXTURE_TYPE: &str = "textureType";

/// Names of the attributes an `app:ParameterizedTexture` states its join in:
/// the polygon a target paints, and the ring one list of texture coordinates
/// belongs to.
const URI_ATTR: &str = "uri";
const RING_ATTR: &str = "ring";

/// The media types CityJSON's two texture formats are written under. The
/// `image/jpg` spelling is not registered with IANA but is common in the
/// wild, and means the same thing as `image/jpeg`.
const MIME_PNG: &str = "image/png";
const MIME_JPEG: &str = "image/jpeg";
const MIME_JPG: &str = "image/jpg";

/// The file extensions those same formats are written under, for an image
/// whose media type the document does not state or states as something else.
const EXT_PNG: &str = ".png";
const EXT_JPG: &str = ".jpg";
const EXT_JPEG: &str = ".jpeg";

/// The number of components in a texture coordinate.
const UV_COMPONENTS: usize = 2;

/// Local names of the properties of an `app:X3DMaterial`, which are the
/// CityJSON `Material` members under different spellings.
const AMBIENT_INTENSITY: &str = "ambientIntensity";
const DIFFUSE_COLOR: &str = "diffuseColor";
const EMISSIVE_COLOR: &str = "emissiveColor";
const SPECULAR_COLOR: &str = "specularColor";
const SHININESS: &str = "shininess";
const TRANSPARENCY: &str = "transparency";
const IS_SMOOTH: &str = "isSmooth";

/// Local name of the GML property naming a feature, which is where an
/// `app:X3DMaterial` gets the name CityJSON requires of a material.
const GML_NAME: &str = "name";

/// The theme of an `app:Appearance` that states none.
///
/// CityGML makes `app:theme` optional, CityJSON makes the theme the *key* of
/// the per-geometry `material` object, and an empty key would be a poor name
/// for the ordinary case. `"default"` is the name the CityJSON specification
/// itself uses for a theme in its examples.
const DEFAULT_THEME: &str = "default";

/// The number of components in a CityJSON colour.
const COLOR_COMPONENTS: usize = 3;

/// One piece of CityGML surface data, ready to be joined to the polygons it
/// names.
#[derive(Debug, Clone, PartialEq)]
pub enum SurfaceData {
    /// An `app:X3DMaterial`: the CityJSON material it becomes, the theme it
    /// was declared under, and the `gml:id` of every polygon it applies to.
    Material {
        theme: String,
        material: MaterialObject,
        /// Polygon `gml:id`s, with the `#` of the `app:target` reference —
        /// and any part before it — stripped off.
        targets: Vec<String>,
    },
    /// An `app:ParameterizedTexture`: the CityJSON texture it becomes, the
    /// theme it was declared under, and one target per polygon it applies to.
    ///
    /// A texture target carries more than a material one: a material paints a
    /// whole polygon, where a texture states one coordinate per *point of
    /// each ring*, so the join needs the ring as well as the polygon.
    Texture {
        theme: String,
        texture: TextureObject,
        targets: Vec<TextureTarget>,
    },
}

/// One `app:target` of an `app:ParameterizedTexture`: the polygon it paints,
/// and the texture coordinates of each of that polygon's rings.
///
/// The coordinates are as the document wrote them, in its own order and
/// without its closing point removed — matching them against the ring the
/// reader repaired is [`crate::convert`]'s job, because only the converter
/// has the geometry.
///
/// CityGML texture space and CityJSON's `vertices-texture` are the same
/// coordinate system — (u, v) in `[0, 1]` from the lower-left corner of the
/// image — so the pairs are passed through unchanged.
#[derive(Debug, Clone, PartialEq)]
pub struct TextureTarget {
    /// The polygon's `gml:id`, from the fragment of the target's `uri`.
    pub polygon_id: String,
    /// Ring `gml:id` and that ring's (u, v) pairs, in document order.
    pub ring_coords: Vec<(String, Vec<[f64; 2]>)>,
}

/// Read every `app:Appearance` reachable from `nodes` into its surface data,
/// in document order.
///
/// Each node may be the `app:Appearance` itself or any element containing
/// one — an `app:appearanceMember` of the `CityModel`, an `app:appearance` of
/// a city object — because both spellings are legal CityGML and the scan is
/// over descendants.
///
/// Surface data that is valid CityGML but has no CityJSON counterpart here —
/// an `app:GeoreferencedTexture`, or a `surfaceDataMember` that only
/// references its content by `xlink:href` — is recorded in `report` rather
/// than dropped in silence.
/// Nothing in an appearance is a hard error: an appearance that cannot be
/// read costs colour, not geometry.
pub fn parse_appearances(nodes: &[XmlNode], report: &mut ParseReport) -> Vec<SurfaceData> {
    let mut out = Vec::new();
    let mut materials = 0;
    for node in nodes {
        for appearance in node
            .descendants()
            .filter(|node| is_in(node, &APPEARANCE_NS, APPEARANCE))
        {
            read_appearance(appearance, &mut materials, &mut out, report);
        }
    }
    out
}

/// Read one `app:Appearance`: its theme, and each of its surface data members.
fn read_appearance(
    node: &XmlNode,
    materials: &mut usize,
    out: &mut Vec<SurfaceData>,
    report: &mut ParseReport,
) {
    let theme = theme_of(node);
    for member in node
        .children
        .iter()
        .filter(|child| is_in(child, &APPEARANCE_NS, SURFACE_DATA_MEMBER))
    {
        if member.children.is_empty() {
            report.skipped.push(skip(
                member,
                format!("<{SURFACE_DATA_MEMBER}> holds no surface data"),
            ));
            continue;
        }
        for data in &member.children {
            if is_in(data, &APPEARANCE_NS, X3D_MATERIAL) {
                let material = read_material(data, *materials, report);
                *materials += 1;
                out.push(SurfaceData::Material {
                    theme: theme.clone(),
                    material,
                    targets: targets_of(data),
                });
            } else if is_in(data, &APPEARANCE_NS, PARAMETERIZED_TEXTURE) {
                if let Some(texture) = read_texture(data, report) {
                    out.push(SurfaceData::Texture {
                        theme: theme.clone(),
                        texture,
                        targets: texture_targets_of(data, report),
                    });
                }
            } else {
                report.skipped.push(skip(
                    data,
                    format!(
                        "<{}> surface data is not converted; only <{X3D_MATERIAL}> and \
                         <{PARAMETERIZED_TEXTURE}> are",
                        data.local
                    ),
                ));
            }
        }
    }
}

/// The theme an appearance declares, or [`DEFAULT_THEME`] where it declares
/// none — an absent `app:theme`, or one that holds nothing but whitespace.
fn theme_of(node: &XmlNode) -> String {
    app_child(node, THEME)
        .map(|theme| theme.text.trim())
        .filter(|theme| !theme.is_empty())
        .unwrap_or(DEFAULT_THEME)
        .to_string()
}

/// One `app:X3DMaterial` as the CityJSON material it becomes.
///
/// `seq` is the position of this material among all the materials read from
/// the document, and is used only to name one whose `gml:name` is missing:
/// CityJSON requires a material to have a name, and a stable generated one
/// beats an empty string.
///
/// A property whose text is not the number, colour or boolean it should be is
/// left out of the material with a warning, rather than defaulted: an invented
/// colour is worse than a missing one.
fn read_material(node: &XmlNode, seq: usize, report: &mut ParseReport) -> MaterialObject {
    MaterialObject {
        name: material_name(node, seq),
        ambient_intensity: number(node, AMBIENT_INTENSITY, report),
        diffuse_color: color(node, DIFFUSE_COLOR, report),
        emissive_color: color(node, EMISSIVE_COLOR, report),
        specular_color: color(node, SPECULAR_COLOR, report),
        shininess: number(node, SHININESS, report),
        transparency: number(node, TRANSPARENCY, report),
        is_smooth: boolean(node, IS_SMOOTH, report),
    }
}

/// The material's `gml:name`, or a generated stand-in.
fn material_name(node: &XmlNode, seq: usize) -> String {
    node.children
        .iter()
        .find(|child| is_gml(child, GML_NAME))
        .map(|name| name.text.trim())
        .filter(|name| !name.is_empty())
        .map_or_else(|| format!("material-{seq}"), str::to_owned)
}

/// The `gml:id` of every polygon an `app:target` of this surface data names.
///
/// A target is a URI reference — `#poly-1` in every file this converter has
/// seen, but `other.gml#poly-1` is a reference too — so everything up to and
/// including the last `#` is dropped and what remains is the id.
fn targets_of(node: &XmlNode) -> Vec<String> {
    node.children
        .iter()
        .filter(|child| is_in(child, &APPEARANCE_NS, TARGET))
        .filter_map(|target| {
            let id = fragment(target.text.trim());
            (!id.is_empty()).then(|| id.to_string())
        })
        .collect()
}

/// The fragment of a URI reference: everything after the last `#`, or the
/// whole thing where there is none.
fn fragment(uri: &str) -> &str {
    match uri.rfind('#') {
        Some(hash) => &uri[hash + 1..],
        None => uri,
    }
}

/// One `app:ParameterizedTexture` as the CityJSON texture it becomes, or
/// `None` — with a skip recorded — when it names no image or no image format
/// this converter can settle.
///
/// CityJSON's `type` is a closed enumeration of `PNG` and `JPG`, so a texture
/// in any other format cannot be written at all; dropping the whole surface
/// data is the honest outcome, and it is recorded rather than silent.
///
/// `app:borderColor` is not read: CityJSON's `borderColor` applies to the
/// `border` wrap mode alone, and nothing in this converter's corpus states
/// one.
fn read_texture(node: &XmlNode, report: &mut ParseReport) -> Option<TextureObject> {
    let Some(image) = app_child(node, IMAGE_URI)
        .map(|uri| uri.text.trim())
        .filter(|uri| !uri.is_empty())
    else {
        report.skipped.push(skip(
            node,
            format!("<{PARAMETERIZED_TEXTURE}> names no <{IMAGE_URI}>"),
        ));
        return None;
    };
    let Some(thetype) = texture_format(node, image) else {
        report.skipped.push(skip(
            node,
            format!(
                "<{PARAMETERIZED_TEXTURE}> image {image:?} is neither a PNG nor a JPEG, \
                 which are the only formats CityJSON states"
            ),
        ));
        return None;
    };
    Some(TextureObject {
        thetype: Some(thetype),
        // Verbatim: the URI is resolved against the document, and rewriting
        // it — even to normalise a separator — would break it.
        image: Some(image.to_string()),
        wrap_mode: enumerated(node, WRAP_MODE, report, |value| {
            serde_json::from_value(value).ok()
        }),
        texture_type: enumerated(node, TEXTURE_TYPE, report, |value| {
            serde_json::from_value(value).ok()
        }),
        border_color: None,
    })
}

/// The image's format: from `app:mimeType` where it states one of the two
/// CityJSON knows, and from the image's file extension otherwise.
///
/// The extension is a fallback and not the first choice: a document that
/// states a media type is stating what the file *is*, where an extension is
/// only what it is called. Both comparisons are case-blind — media types are
/// case-insensitive by RFC 2045, and `ROOF.JPG` is a JPEG.
fn texture_format(node: &XmlNode, image: &str) -> Option<TextureFormat> {
    let mime = app_child(node, MIME_TYPE).map(|mime| mime.text.trim().to_ascii_lowercase());
    let from_mime = match mime.as_deref() {
        Some(MIME_PNG) => Some(TextureFormat::PNG),
        Some(MIME_JPEG | MIME_JPG) => Some(TextureFormat::JPG),
        _ => None,
    };
    from_mime.or_else(|| {
        let image = image.to_ascii_lowercase();
        if image.ends_with(EXT_PNG) {
            Some(TextureFormat::PNG)
        } else if image.ends_with(EXT_JPG) || image.ends_with(EXT_JPEG) {
            Some(TextureFormat::JPG)
        } else {
            None
        }
    })
}

/// One enumerated property of a texture, as the CityJSON enumeration of the
/// same name.
///
/// CityGML and CityJSON spell `wrapMode` and `textureType` identically, so
/// `parse` is cjseq's own deserializer rather than a hand-written `match`:
/// the spelling is then cjseq's to get right, and a value outside the
/// enumeration is dropped with a warning instead of defaulting to a wrong
/// one.
fn enumerated<T>(
    node: &XmlNode,
    local: &str,
    report: &mut ParseReport,
    parse: impl FnOnce(Value) -> Option<T>,
) -> Option<T> {
    let child = app_child(node, local)?;
    let text = child.text.trim();
    match parse(Value::String(text.to_string())) {
        Some(value) => Some(value),
        None => {
            report.warnings.push(format!(
                "<{local}> {text:?} is not a CityJSON {local}; the texture property is dropped"
            ));
            None
        }
    }
}

/// Every `app:target` of a texture, as the polygon it paints and the texture
/// coordinates of that polygon's rings.
///
/// A target this converter cannot use — one naming no polygon, one
/// parameterised by `app:TexCoordGen` rather than by coordinates — is
/// recorded and left out. The texture itself survives: its other targets are
/// still paintable.
fn texture_targets_of(node: &XmlNode, report: &mut ParseReport) -> Vec<TextureTarget> {
    node.children
        .iter()
        .filter(|child| is_in(child, &APPEARANCE_NS, TARGET))
        .filter_map(|target| texture_target(target, report))
        .collect()
}

/// One `app:target` of a texture.
///
/// The `app:TexCoordList` is looked for among the target's descendants rather
/// than its direct children: CityGML wraps the texture parameterization in a
/// property element in some encodings and inlines it in others, and both mean
/// the same thing.
fn texture_target(target: &XmlNode, report: &mut ParseReport) -> Option<TextureTarget> {
    let polygon_id = fragment(target.attr(URI_ATTR).unwrap_or_default().trim());
    if polygon_id.is_empty() {
        report.skipped.push(skip(
            target,
            format!("<{TARGET}> of a texture names no polygon in its {URI_ATTR}"),
        ));
        return None;
    }
    let Some(list) = target
        .descendants()
        .find(|node| is_in(node, &APPEARANCE_NS, TEX_COORD_LIST))
    else {
        let reason = if target
            .descendants()
            .any(|node| is_in(node, &APPEARANCE_NS, TEX_COORD_GEN))
        {
            format!(
                "<{TEX_COORD_GEN}> states a texture as a transformation rather than as \
                 coordinates, which CityJSON cannot express"
            )
        } else {
            format!("<{TARGET}> of a texture holds no <{TEX_COORD_LIST}>")
        };
        report.skipped.push(skip(target, reason));
        return None;
    };
    Some(TextureTarget {
        polygon_id: polygon_id.to_string(),
        ring_coords: list
            .children
            .iter()
            .filter(|child| is_in(child, &APPEARANCE_NS, TEXTURE_COORDINATES))
            .filter_map(|coords| texture_coordinates(coords, report))
            .collect(),
    })
}

/// One `app:textureCoordinates`: the ring it belongs to, and its (u, v)
/// pairs.
///
/// The list is flat, so an odd count is not a list of pairs at all and the
/// ring is left untextured rather than guessed at — as is one holding a
/// coordinate that is not a number, and one naming no ring.
fn texture_coordinates(
    node: &XmlNode,
    report: &mut ParseReport,
) -> Option<(String, Vec<[f64; 2]>)> {
    let ring = fragment(node.attr(RING_ATTR).unwrap_or_default().trim());
    if ring.is_empty() {
        report.skipped.push(skip(
            node,
            format!("<{TEXTURE_COORDINATES}> names no ring in its {RING_ATTR}"),
        ));
        return None;
    }
    let text = node.text.trim();
    let values: Option<Vec<f64>> = text
        .split_ascii_whitespace()
        .map(|token| token.parse::<f64>().ok().filter(|value| value.is_finite()))
        .collect();
    let Some(values) =
        values.filter(|values| !values.is_empty() && values.len() % UV_COMPONENTS == 0)
    else {
        report.skipped.push(skip(
            node,
            format!(
                "<{TEXTURE_COORDINATES}> of ring {ring:?} is not a non-empty, even-length list \
                 of finite numbers"
            ),
        ));
        return None;
    };
    Some((
        ring.to_string(),
        values
            .chunks_exact(UV_COMPONENTS)
            .map(|uv| [uv[0], uv[1]])
            .collect(),
    ))
}

/// A skip naming the element it was recorded for.
fn skip(node: &XmlNode, reason: String) -> Skipped {
    Skipped {
        element: node.local.clone(),
        gml_id: node.gml_id().map(str::to_owned),
        reason,
    }
}

/// One numeric property of a material, if it has one that is a finite number.
fn number(node: &XmlNode, local: &str, report: &mut ParseReport) -> Option<f64> {
    let child = app_child(node, local)?;
    let text = child.text.trim();
    match text.parse::<f64>().ok().filter(|value| value.is_finite()) {
        Some(value) => Some(value),
        None => {
            report.warnings.push(format!(
                "<{local}> {text:?} is not a finite number; the material property is dropped"
            ));
            None
        }
    }
}

/// One colour property of a material: exactly three finite numbers, which is
/// what `appearance.schema.json` pins a CityJSON colour to.
fn color(node: &XmlNode, local: &str, report: &mut ParseReport) -> Option<Color> {
    let child = app_child(node, local)?;
    let text = child.text.trim();
    let components: Option<Vec<f64>> = text
        .split_ascii_whitespace()
        .map(|token| token.parse::<f64>().ok().filter(|value| value.is_finite()))
        .collect();
    match components
        .filter(|components| components.len() == COLOR_COMPONENTS)
        .map(|components| [components[0], components[1], components[2]])
    {
        Some(color) => Some(color),
        None => {
            report.warnings.push(format!(
                "<{local}> {text:?} is not {COLOR_COMPONENTS} finite numbers; \
                 the material property is dropped"
            ));
            None
        }
    }
}

/// One boolean property of a material, in either of the two spellings XML
/// Schema gives a `xs:boolean`.
fn boolean(node: &XmlNode, local: &str, report: &mut ParseReport) -> Option<bool> {
    let child = app_child(node, local)?;
    let text = child.text.trim();
    match text {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => {
            report.warnings.push(format!(
                "<{local}> {text:?} is not a boolean; the material property is dropped"
            ));
            None
        }
    }
}

/// The first direct child that is the named element of the appearance module.
fn app_child<'a>(node: &'a XmlNode, local: &str) -> Option<&'a XmlNode> {
    node.children
        .iter()
        .find(|child| is_in(child, &APPEARANCE_NS, local))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cjseq::{TextureType, WrapMode};
    use pretty_assertions::assert_eq;

    /// The namespaces every fixture below binds.
    const NS: &str = r#"xmlns:app="http://www.opengis.net/citygml/appearance/2.0"
         xmlns:gml="http://www.opengis.net/gml"
         xmlns:xlink="http://www.w3.org/1999/xlink"
         xmlns:other="urn:example:other""#;

    /// Read one XML literal as though it had been collected by the document
    /// scan.
    fn parse(xml: &str) -> (Vec<SurfaceData>, ParseReport) {
        let node = crate::xml::parse_str_for_tests(xml).unwrap();
        let mut report = ParseReport::default();
        let data = parse_appearances(std::slice::from_ref(&node), &mut report);
        (data, report)
    }

    /// An `app:appearanceMember` holding one appearance with `body` in it.
    fn member(body: &str) -> String {
        format!("<app:appearanceMember {NS}><app:Appearance>{body}</app:Appearance></app:appearanceMember>")
    }

    /// The one material of a parse that must have produced exactly one.
    fn only(data: &[SurfaceData]) -> (&str, &MaterialObject, &[String]) {
        assert_eq!(data.len(), 1, "{data:?}");
        match &data[0] {
            SurfaceData::Material {
                theme,
                material,
                targets,
            } => (theme, material, targets),
            other => panic!("expected a material, got {other:?}"),
        }
    }

    /// The one texture of a parse that must have produced exactly one.
    fn only_texture(data: &[SurfaceData]) -> (&str, &TextureObject, &[TextureTarget]) {
        assert_eq!(data.len(), 1, "{data:?}");
        match &data[0] {
            SurfaceData::Texture {
                theme,
                texture,
                targets,
            } => (theme, texture, targets),
            other => panic!("expected a texture, got {other:?}"),
        }
    }

    /// One `app:ParameterizedTexture`'s worth of surface data, in an
    /// appearance of the default theme.
    fn texture(body: &str) -> String {
        member(&format!(
            "<app:surfaceDataMember><app:ParameterizedTexture>{body}\
             </app:ParameterizedTexture></app:surfaceDataMember>"
        ))
    }

    #[test]
    fn an_x3d_material_becomes_a_cityjson_material() {
        let (data, report) = parse(&member(
            r#"<app:theme>summer</app:theme>
               <app:surfaceDataMember>
                 <app:X3DMaterial gml:id="mat-roof">
                   <gml:name>roof-red</gml:name>
                   <app:ambientIntensity>0.4</app:ambientIntensity>
                   <app:diffuseColor>0.9 0.1 0.1</app:diffuseColor>
                   <app:emissiveColor>0 0 0</app:emissiveColor>
                   <app:specularColor>1 1 1</app:specularColor>
                   <app:shininess>0.2</app:shininess>
                   <app:transparency>0.5</app:transparency>
                   <app:isSmooth>true</app:isSmooth>
                   <app:target>#roof-south</app:target>
                   <app:target>#roof-north</app:target>
                 </app:X3DMaterial>
               </app:surfaceDataMember>"#,
        ));
        let (theme, material, targets) = only(&data);
        assert_eq!(theme, "summer");
        assert_eq!(
            material,
            &MaterialObject {
                name: "roof-red".to_string(),
                ambient_intensity: Some(0.4),
                diffuse_color: Some([0.9, 0.1, 0.1]),
                emissive_color: Some([0.0, 0.0, 0.0]),
                specular_color: Some([1.0, 1.0, 1.0]),
                shininess: Some(0.2),
                transparency: Some(0.5),
                is_smooth: Some(true),
            }
        );
        assert_eq!(
            targets,
            ["roof-south".to_string(), "roof-north".to_string()]
        );
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    }

    /// Every material property but the name is optional, and an absent one
    /// must not be invented.
    #[test]
    fn a_material_with_nothing_but_a_name_keeps_nothing_else() {
        let (data, report) = parse(&member(
            r#"<app:surfaceDataMember>
                 <app:X3DMaterial><gml:name>bare</gml:name></app:X3DMaterial>
               </app:surfaceDataMember>"#,
        ));
        let (_, material, targets) = only(&data);
        assert_eq!(
            material,
            &MaterialObject {
                name: "bare".to_string(),
                ambient_intensity: None,
                diffuse_color: None,
                emissive_color: None,
                specular_color: None,
                shininess: None,
                transparency: None,
                is_smooth: None,
            }
        );
        assert!(targets.is_empty());
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    }

    /// CityJSON requires a material to have a name, so one without a
    /// `gml:name` is numbered — by its position among *all* the materials the
    /// document holds, counting from zero, so that the name is stable and
    /// unique across appearances.
    #[test]
    fn a_material_without_a_gml_name_is_numbered() {
        let (data, _) = parse(&member(
            r#"<app:surfaceDataMember>
                 <app:X3DMaterial><gml:name>first</gml:name></app:X3DMaterial>
               </app:surfaceDataMember>
               <app:surfaceDataMember>
                 <app:X3DMaterial/>
               </app:surfaceDataMember>
               <app:surfaceDataMember>
                 <app:X3DMaterial><gml:name>   </gml:name></app:X3DMaterial>
               </app:surfaceDataMember>"#,
        ));
        let names: Vec<&str> = data
            .iter()
            .map(|data| match data {
                SurfaceData::Material { material, .. } => material.name.as_str(),
                other => panic!("expected a material, got {other:?}"),
            })
            .collect();
        assert_eq!(names, vec!["first", "material-1", "material-2"]);
    }

    /// An appearance that names no theme, or names an empty one, is the
    /// default theme: the theme becomes a CityJSON object key, and an empty
    /// key is no name at all.
    #[test]
    fn an_appearance_without_a_theme_is_the_default_theme() {
        for theme in ["", "<app:theme/>", "<app:theme>  </app:theme>"] {
            let (data, _) = parse(&member(&format!(
                "{theme}<app:surfaceDataMember><app:X3DMaterial/></app:surfaceDataMember>"
            )));
            assert_eq!(only(&data).0, DEFAULT_THEME, "{theme}");
        }
    }

    /// Surface data with no CityJSON counterpart here is recorded rather than
    /// dropped in silence — and so is a `surfaceDataMember` whose content is
    /// only referenced.
    #[test]
    fn surface_data_that_is_not_a_material_is_skipped_and_reported() {
        for data_element in [r#"<app:GeoreferencedTexture/>"#, r#"<other:X3DMaterial/>"#] {
            let (data, report) = parse(&member(&format!(
                "<app:surfaceDataMember>{data_element}</app:surfaceDataMember>"
            )));
            assert!(data.is_empty(), "{data_element}: {data:?}");
            assert_eq!(report.skipped.len(), 1, "{data_element}");
        }

        let (data, report) = parse(&member(r##"<app:surfaceDataMember xlink:href="#sd-1"/>"##));
        assert!(data.is_empty(), "{data:?}");
        assert_eq!(report.skipped.len(), 1, "{:?}", report.skipped);
    }

    /// A property that is not the number, colour or boolean it should be is
    /// dropped with a warning: a plausible-looking invented value would be
    /// worse than a missing one.
    #[test]
    fn a_malformed_material_property_is_dropped_with_a_warning() {
        for property in [
            "<app:ambientIntensity>bright</app:ambientIntensity>",
            "<app:shininess>NaN</app:shininess>",
            "<app:transparency/>",
            // Two components, four components, and a component that is not a
            // number: a CityJSON colour is exactly three finite numbers.
            "<app:diffuseColor>0.9 0.1</app:diffuseColor>",
            "<app:diffuseColor>0.9 0.1 0.1 0.1</app:diffuseColor>",
            "<app:diffuseColor>0.9 red 0.1</app:diffuseColor>",
            "<app:isSmooth>yes</app:isSmooth>",
        ] {
            let (data, report) = parse(&member(&format!(
                "<app:surfaceDataMember><app:X3DMaterial>{property}</app:X3DMaterial>\
                 </app:surfaceDataMember>"
            )));
            let (_, material, _) = only(&data);
            assert_eq!(
                material,
                &MaterialObject {
                    name: "material-0".to_string(),
                    ambient_intensity: None,
                    diffuse_color: None,
                    emissive_color: None,
                    specular_color: None,
                    shininess: None,
                    transparency: None,
                    is_smooth: None,
                },
                "{property}"
            );
            assert_eq!(report.warnings.len(), 1, "{property}");
        }

        // `0` and `1` are as much booleans as `false` and `true` are.
        for (text, expected) in [("1", true), ("0", false), ("false", false)] {
            let (data, report) = parse(&member(&format!(
                "<app:surfaceDataMember><app:X3DMaterial>\
                 <app:isSmooth>{text}</app:isSmooth></app:X3DMaterial></app:surfaceDataMember>"
            )));
            assert_eq!(only(&data).1.is_smooth, Some(expected), "{text}");
            assert!(report.warnings.is_empty(), "{text}");
        }
    }

    /// A target is a URI reference: the fragment is the polygon id, and
    /// everything before it — including a document name — is not.
    #[test]
    fn a_target_is_read_as_the_id_its_fragment_names() {
        let (data, _) = parse(&member(
            r#"<app:surfaceDataMember><app:X3DMaterial>
                 <app:target>#poly-1</app:target>
                 <app:target>poly-2</app:target>
                 <app:target>other.gml#poly-3</app:target>
                 <app:target>  #poly-4  </app:target>
                 <app:target/>
                 <app:target>#</app:target>
               </app:X3DMaterial></app:surfaceDataMember>"#,
        ));
        assert_eq!(
            only(&data).2,
            ["poly-1", "poly-2", "poly-3", "poly-4"].map(str::to_string)
        );
    }

    /// The per-object form: CityGML lets a city object carry its own
    /// `app:appearance`, and the appearance inside it is read exactly as one
    /// written at the `CityModel` level.
    #[test]
    fn an_appearance_nested_in_a_city_object_is_read_too() {
        let (data, report) = parse(&format!(
            r#"<bldg:Building {NS} xmlns:bldg="http://www.opengis.net/citygml/building/2.0">
                 <app:appearance>
                   <app:Appearance>
                     <app:theme>winter</app:theme>
                     <app:surfaceDataMember>
                       <app:X3DMaterial><gml:name>snow</gml:name>
                         <app:target>#wall-south</app:target>
                       </app:X3DMaterial>
                     </app:surfaceDataMember>
                   </app:Appearance>
                 </app:appearance>
               </bldg:Building>"#
        ));
        let (theme, material, targets) = only(&data);
        assert_eq!(theme, "winter");
        assert_eq!(material.name, "snow");
        assert_eq!(targets, ["wall-south".to_string()]);
        assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    }

    /// A local name alone identifies nothing: an element of another namespace
    /// that happens to be spelled like an appearance element is not one.
    #[test]
    fn elements_outside_the_appearance_namespace_are_not_appearance() {
        let (data, report) = parse(&format!(
            r#"<other:Appearance {NS}>
                 <other:surfaceDataMember><app:X3DMaterial/></other:surfaceDataMember>
               </other:Appearance>"#
        ));
        assert!(data.is_empty(), "{data:?}");
        assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    }

    //-------------------------------------------------------------------
    //-- textures
    //-------------------------------------------------------------------

    /// An `app:ParameterizedTexture` in full: the image reference, the three
    /// properties CityJSON keeps, and one target per polygon carrying the UVs
    /// of each of that polygon's rings.
    #[test]
    fn a_parameterized_texture_becomes_a_cityjson_texture() {
        let (data, report) = parse(&member(
            r##"<app:theme>rgbTexture</app:theme>
               <app:surfaceDataMember>
                 <app:ParameterizedTexture gml:id="tex-1">
                   <app:imageURI>textures/roof.jpg</app:imageURI>
                   <app:mimeType>image/jpeg</app:mimeType>
                   <app:wrapMode>wrap</app:wrapMode>
                   <app:textureType>specific</app:textureType>
                   <app:target uri="#roof-south">
                     <app:TexCoordList>
                       <app:textureCoordinates ring="#roof-south-outer">
                         0 0 1 0 1 1
                       </app:textureCoordinates>
                       <app:textureCoordinates ring="#roof-south-hole">
                         0.2 0.2 0.4 0.2 0.4 0.4
                       </app:textureCoordinates>
                     </app:TexCoordList>
                   </app:target>
                   <app:target uri="other.gml#roof-north">
                     <app:TexCoordList>
                       <app:textureCoordinates ring="ring-2">0 0 1 0 1 1</app:textureCoordinates>
                     </app:TexCoordList>
                   </app:target>
                 </app:ParameterizedTexture>
               </app:surfaceDataMember>"##,
        ));
        let (theme, texture, targets) = only_texture(&data);
        assert_eq!(theme, "rgbTexture");
        assert_eq!(
            texture,
            &TextureObject {
                thetype: Some(TextureFormat::JPG),
                image: Some("textures/roof.jpg".to_string()),
                wrap_mode: Some(WrapMode::Wrap),
                texture_type: Some(TextureType::Specific),
                border_color: None,
            }
        );
        assert_eq!(
            targets,
            [
                TextureTarget {
                    polygon_id: "roof-south".to_string(),
                    ring_coords: vec![
                        (
                            "roof-south-outer".to_string(),
                            vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]
                        ),
                        (
                            "roof-south-hole".to_string(),
                            vec![[0.2, 0.2], [0.4, 0.2], [0.4, 0.4]]
                        ),
                    ],
                },
                TextureTarget {
                    polygon_id: "roof-north".to_string(),
                    ring_coords: vec![(
                        "ring-2".to_string(),
                        vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]
                    )],
                },
            ]
        );
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    }

    /// The image format comes from `app:mimeType` where there is one this
    /// converter knows, and from the image's file extension where there is
    /// not: CityJSON's `type` is a closed enumeration of `PNG` and `JPG`, and
    /// a texture whose format cannot be settled is no CityJSON texture at all.
    #[test]
    fn the_texture_format_comes_from_the_mime_type_or_the_image_extension() {
        for (mime, uri, expected) in [
            ("image/png", "t/a.jpg", Some(TextureFormat::PNG)),
            ("image/jpeg", "t/a.png", Some(TextureFormat::JPG)),
            ("image/jpg", "t/a", Some(TextureFormat::JPG)),
            // Upper case and surrounding space are the same media type.
            ("  IMAGE/PNG ", "t/a", Some(TextureFormat::PNG)),
            // No usable media type: the extension settles it, case-blind.
            ("image/tiff", "t/a.PNG", Some(TextureFormat::PNG)),
            ("", "t/a.jpeg", Some(TextureFormat::JPG)),
            ("", "http://example.com/t/a.jpg?v=2", None),
            ("", "t/a.tif", None),
            ("", "t/a", None),
        ] {
            let mime = if mime.is_empty() {
                String::new()
            } else {
                format!("<app:mimeType>{mime}</app:mimeType>")
            };
            let (data, report) = parse(&texture(&format!(
                "<app:imageURI>{uri}</app:imageURI>{mime}"
            )));
            match expected {
                Some(format) => {
                    assert_eq!(only_texture(&data).1.thetype, Some(format), "{uri}");
                    assert!(report.skipped.is_empty(), "{uri}: {:?}", report.skipped);
                }
                None => {
                    assert!(data.is_empty(), "{uri}: {data:?}");
                    assert_eq!(report.skipped.len(), 1, "{uri}: {:?}", report.skipped);
                }
            }
        }

        // No image at all is nothing to settle a format against either.
        let (data, report) = parse(&texture("<app:mimeType>image/png</app:mimeType>"));
        assert!(data.is_empty(), "{data:?}");
        assert_eq!(report.skipped.len(), 1, "{:?}", report.skipped);
    }

    /// The image reference is copied verbatim: it is a URI relative to the
    /// document, and rewriting it would break it.
    #[test]
    fn the_image_uri_is_copied_verbatim() {
        let (data, _) = parse(&texture(
            "<app:imageURI>  ../tex/Roof (1).PNG  </app:imageURI>",
        ));
        assert_eq!(
            only_texture(&data).1.image.as_deref(),
            Some("../tex/Roof (1).PNG")
        );
    }

    /// All five CityGML wrap modes are CityJSON wrap modes under the same
    /// spelling; anything else is dropped with a warning rather than
    /// defaulted, because a wrong wrap mode is a visibly wrong texture.
    #[test]
    fn every_wrap_mode_and_texture_type_maps_to_its_cityjson_name() {
        for (text, expected) in [
            ("none", WrapMode::None),
            ("wrap", WrapMode::Wrap),
            ("mirror", WrapMode::Mirror),
            ("clamp", WrapMode::Clamp),
            ("border", WrapMode::Border),
        ] {
            let (data, report) = parse(&texture(&format!(
                "<app:imageURI>a.png</app:imageURI><app:wrapMode>{text}</app:wrapMode>"
            )));
            assert_eq!(only_texture(&data).1.wrap_mode, Some(expected), "{text}");
            assert!(report.warnings.is_empty(), "{text}");
        }

        for (text, expected) in [
            ("unknown", TextureType::Unknown),
            ("specific", TextureType::Specific),
            ("typical", TextureType::Typical),
        ] {
            let (data, report) = parse(&texture(&format!(
                "<app:imageURI>a.png</app:imageURI><app:textureType>{text}</app:textureType>"
            )));
            assert_eq!(only_texture(&data).1.texture_type, Some(expected), "{text}");
            assert!(report.warnings.is_empty(), "{text}");
        }

        for property in [
            "<app:wrapMode>repeat</app:wrapMode>",
            "<app:wrapMode/>",
            "<app:textureType>photo</app:textureType>",
        ] {
            let (data, report) = parse(&texture(&format!(
                "<app:imageURI>a.png</app:imageURI>{property}"
            )));
            let texture = only_texture(&data).1;
            assert_eq!(texture.wrap_mode, None, "{property}");
            assert_eq!(texture.texture_type, None, "{property}");
            assert_eq!(report.warnings.len(), 1, "{property}");
        }
    }

    /// Texture coordinates are (u, v) pairs, so an odd count is not a list of
    /// them: that ring is left untextured and reported, and the rest of the
    /// texture survives.
    #[test]
    fn an_odd_texture_coordinate_count_skips_that_ring() {
        let (data, report) = parse(&texture(
            r##"<app:imageURI>a.png</app:imageURI>
               <app:target uri="#p1">
                 <app:TexCoordList>
                   <app:textureCoordinates ring="#odd">0 0 1 0 1</app:textureCoordinates>
                   <app:textureCoordinates ring="#nan">0 0 1 x</app:textureCoordinates>
                   <app:textureCoordinates ring="#good">0 0 1 0 1 1</app:textureCoordinates>
                   <app:textureCoordinates>0 0 1 0 1 1</app:textureCoordinates>
                 </app:TexCoordList>
               </app:target>"##,
        ));
        let (_, _, targets) = only_texture(&data);
        assert_eq!(
            targets,
            [TextureTarget {
                polygon_id: "p1".to_string(),
                ring_coords: vec![("good".to_string(), vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]])],
            }]
        );
        // One for the odd count, one for the coordinate that is not a number,
        // one for the ring that names no ring.
        assert_eq!(report.skipped.len(), 3, "{:?}", report.skipped);
    }

    /// A target parameterised by `app:TexCoordGen` states the texture as a
    /// transformation into a world file rather than as UVs, which CityJSON
    /// cannot express: that target is skipped and reported. So is a target
    /// naming no polygon.
    #[test]
    fn a_tex_coord_gen_target_is_skipped_and_reported() {
        let (data, report) = parse(&texture(
            r##"<app:imageURI>a.png</app:imageURI>
               <app:target uri="#p1">
                 <app:TexCoordGen>
                   <app:worldToTexture>1 0 0 0 1 0 0 0 1</app:worldToTexture>
                 </app:TexCoordGen>
               </app:target>
               <app:target uri="#">
                 <app:TexCoordList>
                   <app:textureCoordinates ring="#r">0 0 1 0 1 1</app:textureCoordinates>
                 </app:TexCoordList>
               </app:target>"##,
        ));
        assert!(only_texture(&data).2.is_empty(), "{data:?}");
        assert_eq!(report.skipped.len(), 2, "{:?}", report.skipped);
    }

    /// The CityGML 1.0 appearance namespace is read too: files written
    /// against it are still in circulation, and it spells these elements
    /// identically.
    #[test]
    fn the_citygml_1_0_appearance_namespace_is_read_too() {
        let (data, _) = parse(
            r#"<app:appearanceMember xmlns:app="http://www.opengis.net/citygml/appearance/1.0"
                                     xmlns:gml="http://www.opengis.net/gml">
                 <app:Appearance>
                   <app:theme>rgbTexture</app:theme>
                   <app:surfaceDataMember>
                     <app:X3DMaterial><gml:name>old</gml:name>
                       <app:diffuseColor>0.5 0.5 0.5</app:diffuseColor>
                     </app:X3DMaterial>
                   </app:surfaceDataMember>
                 </app:Appearance>
               </app:appearanceMember>"#,
        );
        let (theme, material, _) = only(&data);
        assert_eq!(theme, "rgbTexture");
        assert_eq!(material.diffuse_color, Some([0.5, 0.5, 0.5]));
    }
}
