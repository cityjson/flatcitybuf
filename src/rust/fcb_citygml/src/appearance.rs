//! Appearance: the CityGML appearance module's surface data, as the material
//! palette CityJSON writes.
//!
//! CityGML states appearance *away from* the geometry. An `app:Appearance`
//! holds a theme and a list of surface data — an `app:X3DMaterial`, an
//! `app:ParameterizedTexture` — and each of those names the polygons it
//! applies to by `gml:id`, through one `app:target` per polygon. CityJSON
//! states it the other way round: a document-level palette of materials, and
//! one index per surface written on the geometry that owns the surface.
//!
//! So this module does the reading half only — surface data in, one
//! [`SurfaceData`] per material out, targets kept as the polygon ids they name
//! — and [`crate::convert`] does the join, which is the half that needs the
//! geometry.
//!
//! An `app:Appearance` reaches here from either of the two places CityGML
//! allows it: the `app:appearanceMember` property of the `CityModel`, and the
//! `app:appearance` property of a city object. [`parse_appearances`] takes
//! whatever node each was found as and looks for `app:Appearance` among its
//! descendants, so a caller need not know which of the two it holds.

use cjseq::{Color, MaterialObject};

use crate::gml::is_gml;
use crate::xml::XmlNode;
use crate::{is_in, ParseReport, Skipped, APPEARANCE_NS};

/// Local names of the appearance elements this module reads. As everywhere in
/// this crate, a local name is only ever matched together with a namespace.
const APPEARANCE: &str = "Appearance";
const THEME: &str = "theme";
const SURFACE_DATA_MEMBER: &str = "surfaceDataMember";
const X3D_MATERIAL: &str = "X3DMaterial";
const TARGET: &str = "target";

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
///
/// Textures are not read yet: an `app:ParameterizedTexture` is recorded as
/// skipped rather than turned into a variant of this enum, so that the enum
/// gains its `Texture` case with the code that can fill it in.
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
/// a texture, or a `surfaceDataMember` that only references its content by
/// `xlink:href` — is recorded in `report` rather than dropped in silence.
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
            report.skipped.push(Skipped {
                element: member.local.clone(),
                gml_id: member.gml_id().map(str::to_owned),
                reason: format!("<{SURFACE_DATA_MEMBER}> holds no surface data"),
            });
            continue;
        }
        for data in &member.children {
            if !is_in(data, &APPEARANCE_NS, X3D_MATERIAL) {
                report.skipped.push(Skipped {
                    element: data.local.clone(),
                    gml_id: data.gml_id().map(str::to_owned),
                    reason: format!(
                        "<{}> surface data is not converted; only <{X3D_MATERIAL}> is",
                        data.local
                    ),
                });
                continue;
            }
            let material = read_material(data, *materials, report);
            *materials += 1;
            out.push(SurfaceData::Material {
                theme: theme.clone(),
                material,
                targets: targets_of(data),
            });
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
            let text = target.text.trim();
            let id = match text.rfind('#') {
                Some(hash) => &text[hash + 1..],
                None => text,
            };
            (!id.is_empty()).then(|| id.to_string())
        })
        .collect()
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
        let SurfaceData::Material {
            theme,
            material,
            targets,
        } = &data[0];
        (theme, material, targets)
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
            .map(|SurfaceData::Material { material, .. }| material.name.as_str())
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

    /// Textures are valid CityGML and are not read yet, so they are recorded
    /// rather than dropped in silence — and so is a `surfaceDataMember` whose
    /// content is only referenced.
    #[test]
    fn surface_data_that_is_not_a_material_is_skipped_and_reported() {
        for data_element in [
            r#"<app:ParameterizedTexture gml:id="tex-1"/>"#,
            r#"<app:GeoreferencedTexture/>"#,
            r#"<other:X3DMaterial/>"#,
        ] {
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
