//! Rebuilding a CityJSON geometry from the flat arrays FlatCityBuf stores.
//!
//! **The nesting depth of everything here comes from the geometry type**, which
//! is stored in the `Geometry` table alongside the arrays. Nothing infers depth
//! from which of `solids`/`shells`/`surfaces`/`strings` happen to be populated.
//!
//! That inference is what produced finding #8: `material.values` on a `Solid`
//! with exactly one shell, and `texture.values` on a single-string
//! `MultiLineString`, came back one level deeper than they went in — because a
//! `Solid` and a one-solid `MultiSolid` flatten to byte-identical arrays, and
//! only the type tells them apart.
//!
//! The depths, from `geomprimitives.schema.json` and CityJSON 2.0 §6:
//!
//! | type                              | boundaries | semantics.values | material.values | texture.values |
//! |-----------------------------------|-----------:|-----------------:|----------------:|---------------:|
//! | `MultiPoint`                      |          1 |                1 |     *forbidden* |    *forbidden* |
//! | `MultiLineString`                 |          2 |                1 |     *forbidden* |    *forbidden* |
//! | `MultiSurface`, `CompositeSurface`|          3 |                1 |               1 |              3 |
//! | `Solid`                           |          4 |                2 |               2 |              4 |
//! | `MultiSolid`, `CompositeSolid`    |          5 |                3 |               3 |              5 |
//!
//! `MultiPoint` and `MultiLineString` name neither `material` nor `texture` in
//! their schema and declare `additionalProperties: false`, so a material or
//! texture on one of them is not valid CityJSON and has no depth to decode to.

use cjseq::{
    GeometryType as CjGeometryType, MaterialReference as CjMaterialReference,
    MaterialValues as CjMaterialValues, Ring, SemanticSurfaceType, Semantics, SemanticsSurface,
    SemanticsValues, Shell, Surface, TextureReference as CjTextureReference,
    TextureValues as CjTextureValues, TexturedRing, TexturedShell, TexturedSurface,
};

use crate::error::Error;
use crate::fb::{
    Column, GeometryType, MaterialMapping, SemanticObject, SemanticSurfaceType as FbSurfaceType,
    TextureMapping,
};
use std::collections::HashMap;

use super::deserializer::decode_attributes;

/// The wire spelling of "no value here"; see `writer::geom_encoder::NULL`.
const NULL: u32 = u32::MAX;

fn index(v: u32) -> Option<usize> {
    if v == NULL {
        None
    } else {
        Some(v as usize)
    }
}

/// A cursor over the flattened boundary arrays. Each `take_*` consumes exactly
/// as much as the level above asked for, so the same arrays rebuild any depth.
struct BoundaryCursor<'a> {
    shells: &'a [u32],
    surfaces: &'a [u32],
    strings: &'a [u32],
    indices: &'a [u32],
    shell_cursor: usize,
    surface_cursor: usize,
    string_cursor: usize,
    index_cursor: usize,
}

impl<'a> BoundaryCursor<'a> {
    fn new(shells: &'a [u32], surfaces: &'a [u32], strings: &'a [u32], indices: &'a [u32]) -> Self {
        BoundaryCursor {
            shells,
            surfaces,
            strings,
            indices,
            shell_cursor: 0,
            surface_cursor: 0,
            string_cursor: 0,
            index_cursor: 0,
        }
    }

    fn take_ring(&mut self) -> Ring {
        let size = self.strings.get(self.string_cursor).copied().unwrap_or(0) as usize;
        self.string_cursor += 1;
        let end = (self.index_cursor + size).min(self.indices.len());
        let ring = self.indices[self.index_cursor..end]
            .iter()
            .map(|&i| i as usize)
            .collect();
        self.index_cursor = end;
        ring
    }

    fn take_surface(&mut self) -> Surface {
        let rings = self.surfaces.get(self.surface_cursor).copied().unwrap_or(0);
        self.surface_cursor += 1;
        (0..rings).map(|_| self.take_ring()).collect()
    }

    fn take_shell(&mut self) -> Shell {
        let surfaces = self.shells.get(self.shell_cursor).copied().unwrap_or(0);
        self.shell_cursor += 1;
        (0..surfaces).map(|_| self.take_surface()).collect()
    }
}

/// `MultiPoint`: every index is a point of the one and only ring.
pub(crate) fn decode_points(indices: &[u32]) -> Ring {
    indices.iter().map(|&i| i as usize).collect()
}

/// `MultiLineString`: one ring per `strings` entry.
pub(crate) fn decode_rings(strings: &[u32], indices: &[u32]) -> Vec<Ring> {
    let mut cursor = BoundaryCursor::new(&[], &[], strings, indices);
    (0..strings.len()).map(|_| cursor.take_ring()).collect()
}

/// `MultiSurface`, `CompositeSurface`: one surface per `surfaces` entry.
pub(crate) fn decode_surfaces(surfaces: &[u32], strings: &[u32], indices: &[u32]) -> Vec<Surface> {
    let mut cursor = BoundaryCursor::new(&[], surfaces, strings, indices);
    (0..surfaces.len()).map(|_| cursor.take_surface()).collect()
}

/// `Solid`: one shell per `shells` entry.
pub(crate) fn decode_shells(
    shells: &[u32],
    surfaces: &[u32],
    strings: &[u32],
    indices: &[u32],
) -> Vec<Shell> {
    let mut cursor = BoundaryCursor::new(shells, surfaces, strings, indices);
    (0..shells.len()).map(|_| cursor.take_shell()).collect()
}

/// `MultiSolid`, `CompositeSolid`: `solids[i]` shells in the i-th solid.
pub(crate) fn decode_solids(
    solids: &[u32],
    shells: &[u32],
    surfaces: &[u32],
    strings: &[u32],
    indices: &[u32],
) -> Vec<Vec<Shell>> {
    let mut cursor = BoundaryCursor::new(shells, surfaces, strings, indices);
    solids
        .iter()
        .map(|&n| (0..n).map(|_| cursor.take_shell()).collect())
        .collect()
}

/// Converts FlatBuffers semantic surface objects into CityJSON semantic
/// surfaces.
pub(crate) fn decode_semantics_surfaces(
    semantics_objects: &[SemanticObject],
    semantic_attr_schema: Option<flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<Column<'_>>>>,
) -> Vec<SemanticsSurface> {
    let surfaces = semantics_objects.iter().map(|s| {
        let thetype = to_cj_surface_type(s.type_(), s.extension_type());

        let children = s
            .children()
            .map(|c| c.iter().map(|i| i as usize).collect::<Vec<_>>());

        let attributes = semantic_attr_schema
            .as_ref()
            .and_then(|schema| s.attributes().map(|a| decode_attributes(schema, a)));

        // `other` is the catch-all for members the schema does not name; the
        // encoded attribute blob is exactly that set.
        let other = attributes
            .and_then(|v| match v {
                serde_json::Value::Object(map) => Some(map.into_iter().collect()),
                _ => None,
            })
            .unwrap_or_default();

        SemanticsSurface {
            thetype,
            parent: s.parent().map(|p| p as usize),
            children,
            other,
        }
    });
    surfaces.collect()
}

/// The CityJSON spelling of a FlatBuffers semantic surface type.
///
/// `ExtraSemanticSurface` carries its CityJSON name in `extension_type`, which
/// the spec requires to start with `+`.
///
/// UNKNOWN-TAG POLICY, second of three sites (see [`GeometryType::to_cj`]).
/// Unlike a geometry type, a semantic surface type DOES have an extension
/// escape hatch -- CityJSON § 3.3 says "it is possible to define and use other
/// semantics, but these have to start with a `+`" -- so a tag with no usable
/// `extension_type` still has a schema-valid spelling available, and this
/// emits one rather than erroring. `"+GenericSurface"` and not
/// `"ExtraSemanticSurface"`: the latter is the FlatBuffers enumerator name,
/// is not a CityJSON surface type, and carries no `+`, so a document
/// containing it fails validation. The C++ reader emits the same string.
pub(crate) fn to_cj_surface_type(
    surface_type: FbSurfaceType,
    extension_type: Option<&str>,
) -> SemanticSurfaceType {
    match surface_type {
        FbSurfaceType::RoofSurface => SemanticSurfaceType::RoofSurface,
        FbSurfaceType::GroundSurface => SemanticSurfaceType::GroundSurface,
        FbSurfaceType::WallSurface => SemanticSurfaceType::WallSurface,
        FbSurfaceType::ClosureSurface => SemanticSurfaceType::ClosureSurface,
        FbSurfaceType::OuterCeilingSurface => SemanticSurfaceType::OuterCeilingSurface,
        FbSurfaceType::OuterFloorSurface => SemanticSurfaceType::OuterFloorSurface,
        FbSurfaceType::Window => SemanticSurfaceType::Window,
        FbSurfaceType::Door => SemanticSurfaceType::Door,
        FbSurfaceType::InteriorWallSurface => SemanticSurfaceType::InteriorWallSurface,
        FbSurfaceType::CeilingSurface => SemanticSurfaceType::CeilingSurface,
        FbSurfaceType::FloorSurface => SemanticSurfaceType::FloorSurface,
        FbSurfaceType::WaterSurface => SemanticSurfaceType::WaterSurface,
        FbSurfaceType::WaterGroundSurface => SemanticSurfaceType::WaterGroundSurface,
        FbSurfaceType::WaterClosureSurface => SemanticSurfaceType::WaterClosureSurface,
        FbSurfaceType::TrafficArea => SemanticSurfaceType::TrafficArea,
        FbSurfaceType::AuxiliaryTrafficArea => SemanticSurfaceType::AuxiliaryTrafficArea,
        FbSurfaceType::TransportationMarking => SemanticSurfaceType::TransportationMarking,
        FbSurfaceType::TransportationHole => SemanticSurfaceType::TransportationHole,
        // ExtraSemanticSurface, and any tag a newer writer may add.
        _ => {
            SemanticSurfaceType::Extension(extension_type.unwrap_or("+GenericSurface").to_string())
        }
    }
}

/// Regroups the flat run of semantic values at the depth `geometry_type`
/// implies. `semantics.values` is nested one level less deeply than the
/// geometry's boundaries, so a `Solid` groups by shell and the 5-deep types
/// group by shell and then by solid.
///
/// The group sizes come from the *boundary* count arrays, because a semantics
/// mapping carries none of its own — one semantic value per surface is the
/// whole of its structure.
pub(crate) fn decode_semantics(
    solids: &[u32],
    shells: &[u32],
    geometry_type: GeometryType,
    semantics_objects: Vec<SemanticObject>,
    semantics_values: Option<Vec<u32>>,
    semantic_attr_schema: Option<flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<Column<'_>>>>,
) -> Semantics {
    let surfaces = decode_semantics_surfaces(&semantics_objects, semantic_attr_schema);

    // No values vector at all is `"values": null` -- a member whose value is
    // null, which the schema requires to be present and permits to be null.
    let Some(semantics_values) = semantics_values else {
        return Semantics {
            surfaces,
            values: None,
            other: HashMap::new(),
        };
    };

    let mut cursor = 0usize;
    let mut take_shell = |n: usize, values: &[u32]| -> Vec<Option<usize>> {
        let end = (cursor + n).min(values.len());
        let out = values[cursor..end].iter().map(|&v| index(v)).collect();
        cursor = end;
        out
    };

    let values = match geometry_type {
        // One value per surface, flat.
        GeometryType::MultiPoint
        | GeometryType::MultiLineString
        | GeometryType::MultiSurface
        | GeometryType::CompositeSurface => {
            SemanticsValues::Surfaces(semantics_values.iter().map(|&v| index(v)).collect())
        }
        // One array per shell.
        GeometryType::Solid => SemanticsValues::Shells(
            shells
                .iter()
                .map(|&n| Some(take_shell(n as usize, &semantics_values)))
                .collect(),
        ),
        // One array per shell, per solid.
        GeometryType::MultiSolid | GeometryType::CompositeSolid => {
            let mut shell_cursor = 0usize;
            SemanticsValues::Solids(
                solids
                    .iter()
                    .map(|&shell_count| {
                        Some(
                            (0..shell_count)
                                .map(|_| {
                                    let n = shells.get(shell_cursor).copied().unwrap_or(0) as usize;
                                    shell_cursor += 1;
                                    Some(take_shell(n, &semantics_values))
                                })
                                .collect(),
                        )
                    })
                    .collect(),
            )
        }
        // A GeometryInstance carries no semantics; its template does.
        _ => SemanticsValues::Surfaces(semantics_values.iter().map(|&v| index(v)).collect()),
    };

    Semantics {
        surfaces,
        values: Some(values),
        other: HashMap::new(),
    }
}

/// UNKNOWN-TAG POLICY, first of three sites. See `to_cj_surface_type` and
/// `deserializer::to_cj_co_type` for the other two.
///
/// A geometry type is the one of the three that has NO extension escape
/// hatch: CityJSON § 3 enumerates exactly eight `type` values and
/// `geomprimitives.schema.json` admits no others, so unlike a City Object or
/// a semantic surface there is no `"+Something"` a reader could legally emit
/// for a tag it does not recognise. That leaves two options, and only two:
/// mislabel the geometry as one of the eight, or refuse the file.
///
/// This errors. A tag outside the eight means the file was written by a newer
/// or a broken encoder, and calling such a geometry a `Solid` -- which is
/// what this used to do -- decodes its boundaries at the wrong depth and hands
/// the caller a plausible-looking lie. That is the same reasoning behind
/// [`crate::error::Error::UnknownEnumTag`] for `wrapMode` and `textureType`,
/// and the C++ reader's `geometry_type_name` has always thrown here; the two
/// readers now agree.
impl GeometryType {
    pub fn to_str(self) -> Result<&'static str, Error> {
        Ok(match self {
            Self::MultiPoint => "MultiPoint",
            Self::MultiLineString => "MultiLineString",
            Self::MultiSurface => "MultiSurface",
            Self::CompositeSurface => "CompositeSurface",
            Self::Solid => "Solid",
            Self::MultiSolid => "MultiSolid",
            Self::CompositeSolid => "CompositeSolid",
            Self::GeometryInstance => "GeometryInstance",
            other => return Err(Error::UnknownEnumTag("GeometryType", format!("{other:?}"))),
        })
    }

    pub fn to_cj(self) -> Result<CjGeometryType, Error> {
        Ok(match self {
            Self::MultiPoint => CjGeometryType::MultiPoint,
            Self::MultiLineString => CjGeometryType::MultiLineString,
            Self::MultiSurface => CjGeometryType::MultiSurface,
            Self::CompositeSurface => CjGeometryType::CompositeSurface,
            Self::Solid => CjGeometryType::Solid,
            Self::MultiSolid => CjGeometryType::MultiSolid,
            Self::CompositeSolid => CjGeometryType::CompositeSolid,
            Self::GeometryInstance => CjGeometryType::GeometryInstance,
            other => return Err(Error::UnknownEnumTag("GeometryType", format!("{other:?}"))),
        })
    }
}

/// Rebuilds `material.values` at the depth `geometry_type` implies.
///
/// `material.values` is nested two levels less deeply than the boundaries, so
/// a `MultiSurface` gets one index per surface, a `Solid` one array per shell,
/// and a `MultiSolid`/`CompositeSolid` one array per shell per solid.
///
/// A `NULL` entry in `shells`/`solids` is a whole `null` shell or solid, which
/// `material.values` permits at every level; it comes back as `None`, never as
/// an empty array.
pub(crate) fn decode_materials(
    geometry_type: GeometryType,
    material_mappings: &[MaterialMapping],
) -> Option<HashMap<String, CjMaterialReference>> {
    if material_mappings.is_empty() {
        return None;
    }

    let mut materials = HashMap::new();

    for mapping in material_mappings {
        let theme = mapping.theme().unwrap_or("theme").to_string();

        // A `value` colours the whole object and has no depth at all.
        if let Some(value) = mapping.value() {
            materials.insert(
                theme,
                CjMaterialReference {
                    value: Some(value as usize),
                    values: None,
                    other: HashMap::new(),
                },
            );
            continue;
        }

        let solids = mapping
            .solids()
            .map(|s| s.iter().collect::<Vec<_>>())
            .unwrap_or_default();
        let shells = mapping
            .shells()
            .map(|s| s.iter().collect::<Vec<_>>())
            .unwrap_or_default();
        // No `vertices` vector at all is `"values": null` — an explicit null,
        // which the schema distinguishes from an absent `values`.
        let Some(vertices) = mapping.vertices().map(|v| v.iter().collect::<Vec<_>>()) else {
            materials.insert(
                theme,
                CjMaterialReference {
                    value: None,
                    values: Some(None),
                    other: HashMap::new(),
                },
            );
            continue;
        };

        let mut vertex_cursor = 0usize;
        let mut take_shell = |n: usize| -> Vec<Option<usize>> {
            let end = (vertex_cursor + n).min(vertices.len());
            let out = vertices[vertex_cursor..end]
                .iter()
                .map(|&v| index(v))
                .collect();
            vertex_cursor = end;
            out
        };

        let values = match geometry_type {
            // One index per surface.
            GeometryType::MultiSurface | GeometryType::CompositeSurface => {
                CjMaterialValues::Surfaces(vertices.iter().map(|&v| index(v)).collect())
            }
            // One array per shell.
            GeometryType::Solid => CjMaterialValues::Shells(
                shells
                    .iter()
                    .map(|&n| {
                        if n == NULL {
                            None
                        } else {
                            Some(take_shell(n as usize))
                        }
                    })
                    .collect(),
            ),
            // One array per shell, per solid.
            GeometryType::MultiSolid | GeometryType::CompositeSolid => {
                let mut shell_cursor = 0usize;
                CjMaterialValues::Solids(
                    solids
                        .iter()
                        .map(|&shell_count| {
                            if shell_count == NULL {
                                return None;
                            }
                            Some(
                                (0..shell_count)
                                    .map(|_| {
                                        let n = shells.get(shell_cursor).copied().unwrap_or(0);
                                        shell_cursor += 1;
                                        if n == NULL {
                                            None
                                        } else {
                                            Some(take_shell(n as usize))
                                        }
                                    })
                                    .collect(),
                            )
                        })
                        .collect(),
                )
            }
            // MultiPoint, MultiLineString and GeometryInstance cannot carry a
            // material; if one is somehow present it has no depth of its own,
            // so it is read as the shallowest thing it could be.
            _ => CjMaterialValues::Surfaces(vertices.iter().map(|&v| index(v)).collect()),
        };

        materials.insert(
            theme,
            CjMaterialReference {
                value: None,
                values: Some(Some(values)),
                other: HashMap::new(),
            },
        );
    }

    Some(materials)
}

/// Rebuilds `texture.values` at the depth `geometry_type` implies.
///
/// `texture.values` is nested exactly as deeply as the geometry's boundaries,
/// each ring becoming `[texture_index, uv_index, ...]`. Unlike a material, only
/// the leaf is nullable — the schema types every intermediate level as a plain
/// `"array"` — so nothing here decodes an intermediate `null`.
pub(crate) fn decode_textures(
    geometry_type: GeometryType,
    texture_mappings: &[TextureMapping],
) -> Option<HashMap<String, CjTextureReference>> {
    if texture_mappings.is_empty() {
        return None;
    }

    let mut textures = HashMap::new();

    for mapping in texture_mappings {
        let theme = mapping.theme().unwrap_or("theme").to_string();

        // A theme may legally carry no `values` at all, and is then written
        // with no arrays — distinct from a theme whose `values` is empty.
        let Some(vertices) = mapping.vertices().map(|v| v.iter().collect::<Vec<_>>()) else {
            textures.insert(
                theme,
                CjTextureReference {
                    values: None,
                    other: HashMap::new(),
                },
            );
            continue;
        };
        let solids = mapping
            .solids()
            .map(|s| s.iter().collect::<Vec<_>>())
            .unwrap_or_default();
        let shells = mapping
            .shells()
            .map(|s| s.iter().collect::<Vec<_>>())
            .unwrap_or_default();
        let surfaces = mapping
            .surfaces()
            .map(|s| s.iter().collect::<Vec<_>>())
            .unwrap_or_default();
        let strings = mapping
            .strings()
            .map(|s| s.iter().collect::<Vec<_>>())
            .unwrap_or_default();

        let mut cursor = TextureCursor {
            surfaces: &surfaces,
            shells: &shells,
            strings: &strings,
            vertices: &vertices,
            shell_cursor: 0,
            surface_cursor: 0,
            string_cursor: 0,
            vertex_cursor: 0,
        };

        let values = match geometry_type {
            // Per surface, per ring.
            GeometryType::MultiSurface | GeometryType::CompositeSurface => {
                CjTextureValues::Surface(
                    (0..surfaces.len()).map(|_| cursor.take_surface()).collect(),
                )
            }
            // ... per shell.
            GeometryType::Solid => {
                CjTextureValues::Shell((0..shells.len()).map(|_| cursor.take_shell()).collect())
            }
            // ... per solid.
            GeometryType::MultiSolid | GeometryType::CompositeSolid => CjTextureValues::Solid(
                solids
                    .iter()
                    .map(|&n| (0..n).map(|_| cursor.take_shell()).collect())
                    .collect(),
            ),
            // MultiPoint, MultiLineString and GeometryInstance cannot carry a
            // texture; read whatever is there at the shallowest legal depth.
            _ => CjTextureValues::Surface(
                (0..surfaces.len().max(1))
                    .map(|_| cursor.take_surface())
                    .collect(),
            ),
        };

        textures.insert(
            theme,
            CjTextureReference {
                values: Some(values),
                other: HashMap::new(),
            },
        );
    }

    Some(textures)
}

/// The texture equivalent of [`BoundaryCursor`]: the same four count arrays,
/// but the leaf holds `[texture_index, uv_index, ...]` rather than vertex
/// indices, and is nullable.
struct TextureCursor<'a> {
    surfaces: &'a [u32],
    shells: &'a [u32],
    strings: &'a [u32],
    vertices: &'a [u32],
    shell_cursor: usize,
    surface_cursor: usize,
    string_cursor: usize,
    vertex_cursor: usize,
}

impl TextureCursor<'_> {
    fn take_ring(&mut self) -> TexturedRing {
        let size = self.strings.get(self.string_cursor).copied().unwrap_or(0) as usize;
        self.string_cursor += 1;
        let end = (self.vertex_cursor + size).min(self.vertices.len());
        let ring = self.vertices[self.vertex_cursor..end]
            .iter()
            .map(|&v| index(v))
            .collect();
        self.vertex_cursor = end;
        ring
    }

    fn take_surface(&mut self) -> TexturedSurface {
        let rings = self.surfaces.get(self.surface_cursor).copied().unwrap_or(0);
        self.surface_cursor += 1;
        (0..rings).map(|_| self.take_ring()).collect()
    }

    fn take_shell(&mut self) -> TexturedShell {
        let surfaces = self.shells.get(self.shell_cursor).copied().unwrap_or(0);
        self.shell_cursor += 1;
        (0..surfaces).map(|_| self.take_surface()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fb::geometry_generated::{
        MaterialMappingArgs, TextureMapping as FbTextureMapping, TextureMappingArgs,
    };
    use anyhow::Result;
    use flatbuffers::FlatBufferBuilder;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    /// Builds one FlatBuffers `MaterialMapping` from the raw arrays and runs it
    /// through `decode_materials` at `geometry_type`, returning the decoded
    /// `values` as JSON so expectations read as CityJSON.
    fn decode_material_values(
        geometry_type: GeometryType,
        solids: &[u32],
        shells: &[u32],
        vertices: &[u32],
    ) -> serde_json::Value {
        let mut fbb = FlatBufferBuilder::new();
        let theme = fbb.create_string("t");
        let solids_v = (!solids.is_empty()).then(|| fbb.create_vector(solids));
        let shells_v = (!shells.is_empty()).then(|| fbb.create_vector(shells));
        let vertices_v = fbb.create_vector(vertices);
        let mapping = MaterialMapping::create(
            &mut fbb,
            &MaterialMappingArgs {
                theme: Some(theme),
                solids: solids_v,
                shells: shells_v,
                vertices: Some(vertices_v),
                value: None,
            },
        );
        fbb.finish(mapping, None);
        let buf = fbb.finished_data().to_vec();
        let mapping = flatbuffers::root::<MaterialMapping>(&buf).expect("valid mapping");
        let decoded = decode_materials(geometry_type, &[mapping]).expect("one theme");
        serde_json::to_value(&decoded["t"].values).expect("values serialize")
    }

    fn decode_texture_values(
        geometry_type: GeometryType,
        solids: &[u32],
        shells: &[u32],
        surfaces: &[u32],
        strings: &[u32],
        vertices: &[u32],
    ) -> serde_json::Value {
        let mut fbb = FlatBufferBuilder::new();
        let theme = fbb.create_string("t");
        let solids_v = fbb.create_vector(solids);
        let shells_v = fbb.create_vector(shells);
        let surfaces_v = fbb.create_vector(surfaces);
        let strings_v = fbb.create_vector(strings);
        let vertices_v = fbb.create_vector(vertices);
        let mapping = FbTextureMapping::create(
            &mut fbb,
            &TextureMappingArgs {
                theme: Some(theme),
                solids: Some(solids_v),
                shells: Some(shells_v),
                surfaces: Some(surfaces_v),
                strings: Some(strings_v),
                vertices: Some(vertices_v),
            },
        );
        fbb.finish(mapping, None);
        let buf = fbb.finished_data().to_vec();
        let mapping = flatbuffers::root::<FbTextureMapping>(&buf).expect("valid mapping");
        let decoded = decode_textures(geometry_type, &[mapping]).expect("one theme");
        serde_json::to_value(&decoded["t"].values).expect("values serialize")
    }

    #[test]
    fn test_decode_boundaries() -> Result<()> {
        // MultiPoint
        assert_eq!(decode_points(&[2, 44, 0, 7]), vec![2, 44, 0, 7]);

        // MultiLineString
        assert_eq!(
            serde_json::to_value(decode_rings(&[3, 3], &[2, 3, 5, 77, 55, 212]))?,
            json!([[2, 3, 5], [77, 55, 212]])
        );

        // MultiSurface
        assert_eq!(
            serde_json::to_value(decode_surfaces(
                &[1, 1, 1],
                &[4, 4, 4],
                &[0, 3, 2, 1, 4, 5, 6, 7, 0, 1, 5, 4]
            ))?,
            json!([[[0, 3, 2, 1]], [[4, 5, 6, 7]], [[0, 1, 5, 4]]])
        );

        // Solid
        let indices = [
            0, 3, 2, 1, 22, 1, 2, 3, 4, 4, 5, 6, 7, 0, 1, 5, 4, 1, 2, 6, 5, 240, 243, 124, 244,
            246, 724, 34, 414, 45, 111, 246, 5,
        ];
        assert_eq!(
            serde_json::to_value(decode_shells(
                &[4, 4],
                &[2, 1, 1, 1, 1, 1, 1, 1],
                &[5, 4, 4, 4, 4, 3, 3, 3, 3],
                &indices
            ))?,
            json!([
                [
                    [[0, 3, 2, 1, 22], [1, 2, 3, 4]],
                    [[4, 5, 6, 7]],
                    [[0, 1, 5, 4]],
                    [[1, 2, 6, 5]]
                ],
                [
                    [[240, 243, 124]],
                    [[244, 246, 724]],
                    [[34, 414, 45]],
                    [[111, 246, 5]]
                ]
            ])
        );

        // CompositeSolid
        let indices = [
            0, 3, 2, 1, 22, 4, 5, 6, 7, 0, 1, 5, 4, 1, 2, 6, 5, 240, 243, 124, 244, 246, 724, 34,
            414, 45, 111, 246, 5, 666, 667, 668, 74, 75, 76, 880, 881, 885, 111, 122, 226,
        ];
        assert_eq!(
            serde_json::to_value(decode_solids(
                &[2, 1],
                &[4, 4, 4],
                &[1; 12],
                &[5, 4, 4, 4, 3, 3, 3, 3, 3, 3, 3, 3],
                &indices
            ))?,
            json!([
                [
                    [
                        [[0, 3, 2, 1, 22]],
                        [[4, 5, 6, 7]],
                        [[0, 1, 5, 4]],
                        [[1, 2, 6, 5]]
                    ],
                    [
                        [[240, 243, 124]],
                        [[244, 246, 724]],
                        [[34, 414, 45]],
                        [[111, 246, 5]]
                    ]
                ],
                [[
                    [[666, 667, 668]],
                    [[74, 75, 76]],
                    [[880, 881, 885]],
                    [[111, 122, 226]]
                ]]
            ])
        );

        Ok(())
    }

    /// A `Solid` and a one-solid `MultiSolid` flatten to byte-identical
    /// material arrays. Only the geometry type tells them apart, and it must:
    /// this is finding #8 in one assertion.
    #[test]
    fn identical_material_arrays_decode_to_different_depths_per_type() {
        let (solids, shells, vertices) = (&[1u32][..], &[2u32][..], &[0u32, 1][..]);

        assert_eq!(
            decode_material_values(GeometryType::Solid, solids, shells, vertices),
            json!([[0, 1]]),
            "a Solid's material values are one array per shell"
        );
        assert_eq!(
            decode_material_values(GeometryType::MultiSolid, solids, shells, vertices),
            json!([[[0, 1]]]),
            "a MultiSolid's are one array per shell, per solid"
        );
        assert_eq!(
            decode_material_values(GeometryType::CompositeSolid, solids, shells, vertices),
            json!([[[0, 1]]]),
            "a CompositeSolid decodes exactly as a MultiSolid does"
        );
    }

    #[test]
    fn test_decode_materials() -> Result<()> {
        // A `value` colours the whole object, whatever the geometry type.
        let mut fbb = FlatBufferBuilder::new();
        let theme = fbb.create_string("theme1");
        let mapping = MaterialMapping::create(
            &mut fbb,
            &MaterialMappingArgs {
                theme: Some(theme),
                value: Some(5),
                ..Default::default()
            },
        );
        fbb.finish(mapping, None);
        let buf = fbb.finished_data().to_vec();
        let mapping = flatbuffers::root::<MaterialMapping>(&buf)?;
        let materials = decode_materials(GeometryType::Solid, &[mapping]).expect("one theme");
        assert_eq!(materials["theme1"].value, Some(5));
        assert!(materials["theme1"].values.is_none());

        // MultiSurface: one index per surface.
        assert_eq!(
            decode_material_values(GeometryType::MultiSurface, &[], &[], &[0, 1, NULL, 2]),
            json!([0, 1, null, 2])
        );
        assert_eq!(
            decode_material_values(GeometryType::CompositeSurface, &[], &[], &[0, 1, NULL, 2]),
            json!([0, 1, null, 2])
        );

        // Solid: one array per shell.
        assert_eq!(
            decode_material_values(GeometryType::Solid, &[2], &[3, 3], &[0, 1, NULL, 2, 3, 4]),
            json!([[0, 1, null], [2, 3, 4]])
        );

        // CompositeSolid: one array per shell, per solid.
        assert_eq!(
            decode_material_values(
                GeometryType::CompositeSolid,
                &[2, 1],
                &[3, 3, 3],
                &[0, 1, NULL, 2, NULL, NULL, 3, 4, NULL]
            ),
            json!([[[0, 1, null], [2, null, null]], [[3, 4, null]]])
        );

        Ok(())
    }

    /// `material.values` is nullable at every level. A `NULL` count is a whole
    /// `null` shell or solid and must come back as `null`, never as `[]`
    /// (finding #7).
    #[test]
    fn a_null_material_shell_or_solid_decodes_as_null() {
        assert_eq!(
            decode_material_values(GeometryType::Solid, &[2], &[2, NULL], &[0, 1]),
            json!([[0, 1], null])
        );
        assert_eq!(
            decode_material_values(GeometryType::CompositeSolid, &[1, NULL], &[2], &[0, 1]),
            json!([[[0, 1]], null])
        );
    }

    #[test]
    fn test_decode_textures() -> Result<()> {
        // MultiSurface: per surface, per ring.
        assert_eq!(
            decode_texture_values(
                GeometryType::MultiSurface,
                &[],
                &[3],
                &[1, 1, 1],
                &[4, 4, 4],
                &[0, 10, 20, 30, 1, 11, 21, NULL, 2, 12, NULL, 32]
            ),
            json!([[[0, 10, 20, 30]], [[1, 11, 21, null]], [[2, 12, null, 32]]])
        );

        // Solid: ... per shell.
        assert_eq!(
            decode_texture_values(
                GeometryType::Solid,
                &[2],
                &[3, 2],
                &[1, 1, 1, 1, 1],
                &[4, 4, 4, 4, 4],
                &[0, 10, 20, 30, 1, 11, 21, NULL, 2, 12, NULL, 32, 3, 13, 23, 33, 4, 14, 24, NULL]
            ),
            json!([
                [[[0, 10, 20, 30]], [[1, 11, 21, null]], [[2, 12, null, 32]]],
                [[[3, 13, 23, 33]], [[4, 14, 24, null]]]
            ])
        );

        // CompositeSolid: ... per solid.
        assert_eq!(
            decode_texture_values(
                GeometryType::CompositeSolid,
                &[2, 1],
                &[2, 2, 2],
                &[1; 6],
                &[3; 6],
                &[0, 10, 20, 1, 11, NULL, 2, 12, 22, 3, NULL, 23, 4, 14, 24, 5, 15, 25]
            ),
            json!([
                [
                    [[[0, 10, 20]], [[1, 11, null]]],
                    [[[2, 12, 22]], [[3, null, 23]]]
                ],
                [[[[4, 14, 24]], [[5, 15, 25]]]]
            ])
        );

        Ok(())
    }

    /// The texture equivalent of the material assertion above: a `Solid` and a
    /// one-solid `MultiSolid` emit identical texture arrays.
    #[test]
    fn identical_texture_arrays_decode_to_different_depths_per_type() {
        let args = (
            &[1u32][..],
            &[1u32][..],
            &[1u32][..],
            &[3u32][..],
            &[0u32, 10, 20][..],
        );
        assert_eq!(
            decode_texture_values(GeometryType::Solid, args.0, args.1, args.2, args.3, args.4),
            json!([[[[0, 10, 20]]]])
        );
        assert_eq!(
            decode_texture_values(
                GeometryType::MultiSolid,
                args.0,
                args.1,
                args.2,
                args.3,
                args.4
            ),
            json!([[[[[0, 10, 20]]]]])
        );
    }

    // ---------------------------------------------------------------------
    // UNKNOWN-TAG POLICY. One policy per tag, and the C++ reader agrees on
    // all three (src/cpp/src/cityjson.cpp, src/cpp/src/geometry.cpp; pinned
    // by test_cityjson.cpp and test_geometry.cpp).
    // ---------------------------------------------------------------------

    /// A geometry type has NO '+'-prefixed extension form -- CityJSON section 3
    /// enumerates exactly eight `type` values -- so there is no schema-valid
    /// string to fall back to and an unknown tag is an error. It used to
    /// become a `Solid`, which reads the boundaries at the wrong depth and
    /// hands the caller a plausible-looking lie.
    #[test]
    fn an_unknown_geometry_tag_is_an_error_and_never_a_solid() {
        // every one of the eight still resolves
        for (tag, name) in [
            (GeometryType::MultiPoint, "MultiPoint"),
            (GeometryType::MultiLineString, "MultiLineString"),
            (GeometryType::MultiSurface, "MultiSurface"),
            (GeometryType::CompositeSurface, "CompositeSurface"),
            (GeometryType::Solid, "Solid"),
            (GeometryType::MultiSolid, "MultiSolid"),
            (GeometryType::CompositeSolid, "CompositeSolid"),
            (GeometryType::GeometryInstance, "GeometryInstance"),
        ] {
            assert_eq!(tag.to_str().unwrap(), name);
            assert!(tag.to_cj().is_ok());
        }

        // a tag past the eight is rejected, not silently renamed
        let unknown = GeometryType(GeometryType::ENUM_MAX + 1);
        assert!(
            matches!(
                unknown.to_str(),
                Err(Error::UnknownEnumTag("GeometryType", _))
            ),
            "an unknown geometry tag must be reported, not spelled `Solid`"
        );
        assert!(matches!(
            unknown.to_cj(),
            Err(Error::UnknownEnumTag("GeometryType", _))
        ));
    }

    /// A semantic surface type DOES have an extension form (section 3.3: "it
    /// is possible to define and use other semantics, but these have to start
    /// with a `+`"), so an unnameable tag gets a schema-valid placeholder
    /// rather than an error. Never `"ExtraSemanticSurface"`: that is the
    /// FlatBuffers enumerator name, is not a CityJSON surface type, and
    /// carries no `+`.
    #[test]
    fn an_unnameable_semantic_surface_tag_becomes_a_plus_prefixed_extension() {
        // the extension_type string wins whenever it is there
        assert_eq!(
            to_cj_surface_type(FbSurfaceType::ExtraSemanticSurface, Some("+ThermalSurface")),
            SemanticSurfaceType::Extension("+ThermalSurface".to_string())
        );

        for tag in [
            FbSurfaceType::ExtraSemanticSurface,
            FbSurfaceType(FbSurfaceType::ENUM_MAX + 1),
        ] {
            let SemanticSurfaceType::Extension(name) = to_cj_surface_type(tag, None) else {
                panic!("an unnameable surface tag must become an Extension");
            };
            assert_eq!(name, "+GenericSurface");
            assert!(
                name.starts_with('+'),
                "{name} must be a valid Extension name"
            );
            assert_ne!(name, "ExtraSemanticSurface");
        }
    }
}
