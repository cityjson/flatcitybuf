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
    GeometryType as CjGeometryType, Index, MaterialIndex, MaterialReference as CjMaterialReference,
    MultiSolidValues, PrimitiveValues, Ring, SemanticSurfaceType, Semantics, SemanticsSurface,
    Shell, SolidValues, Surface, SurfaceIndex, TextureReference as CjTextureReference,
    TexturedRing, TexturedShell, TexturedSurface, Themes, VertexIndex,
};

use crate::error::Error;
use crate::fb::{
    Column, GeometryType, MaterialMapping, SemanticObject, SemanticSurfaceType as FbSurfaceType,
    TextureMapping,
};

use super::deserializer::decode_attributes;

/// The wire spelling of "no value here"; see `writer::geom_encoder::NULL`.
const NULL: u32 = u32::MAX;

fn index<I: Index>(v: u32) -> Option<I> {
    if v == NULL {
        None
    } else {
        Some(I::new(v as usize))
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
            .map(|&i| VertexIndex(i as usize))
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
    indices.iter().map(|&i| VertexIndex(i as usize)).collect()
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

        let children = s.children().map(|c| {
            c.iter()
                .map(|i| SurfaceIndex(i as usize))
                .collect::<Vec<_>>()
        });

        let attributes = semantic_attr_schema
            .as_ref()
            .and_then(|schema| s.attributes().map(|a| decode_attributes(schema, a)));

        // `other` is the catch-all for members the schema does not name; the
        // encoded attribute blob is exactly that set.
        let other = attributes
            .and_then(|v| match v {
                serde_json::Value::Object(map) => Some(map),
                _ => None,
            })
            .unwrap_or_default();

        SemanticsSurface {
            thetype,
            parent: s.parent().map(|p| SurfaceIndex(p as usize)),
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

/// The three shapes `semantics.values` and `material.values` come in, and how
/// each is rebuilt from a flat run of leaves and the counts that group it.
/// The shape — and with it the depth — is the one the geometry variant names,
/// so the caller's type decides it, never the arrays.
///
/// A `NULL` entry in `shells` or `solids` is a whole `null` shell or solid; it
/// comes back as `None`, never as an empty array. Boundary counts, which a
/// semantics mapping is regrouped by, are never `NULL`.
pub(crate) trait UnflattenValues<I>: Sized {
    fn unflatten(solids: &[u32], shells: &[u32], leaves: &[u32]) -> Self;
}

/// Hands out consecutive runs of a flat leaf array.
struct LeafCursor<'a> {
    leaves: &'a [u32],
    cursor: usize,
}

impl LeafCursor<'_> {
    fn take<I: Index>(&mut self, n: u32) -> Option<PrimitiveValues<I>> {
        if n == NULL {
            return None;
        }
        let end = (self.cursor + n as usize).min(self.leaves.len());
        let out = self.leaves[self.cursor..end]
            .iter()
            .map(|&v| index(v))
            .collect();
        self.cursor = end;
        Some(out)
    }
}

impl<I: Index> UnflattenValues<I> for PrimitiveValues<I> {
    fn unflatten(_: &[u32], _: &[u32], leaves: &[u32]) -> Self {
        leaves.iter().map(|&v| index(v)).collect()
    }
}

impl<I: Index> UnflattenValues<I> for SolidValues<I> {
    fn unflatten(_: &[u32], shells: &[u32], leaves: &[u32]) -> Self {
        let mut leaves = LeafCursor { leaves, cursor: 0 };
        shells.iter().map(|&n| leaves.take(n)).collect()
    }
}

impl<I: Index> UnflattenValues<I> for MultiSolidValues<I> {
    fn unflatten(solids: &[u32], shells: &[u32], leaves: &[u32]) -> Self {
        let mut leaves = LeafCursor { leaves, cursor: 0 };
        let mut shell_cursor = 0usize;
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
                            leaves.take(n)
                        })
                        .collect(),
                )
            })
            .collect()
    }
}

/// Regroups the flat run of semantic values into the shape `V` the geometry
/// variant names. `semantics.values` is nested one level less deeply than the
/// geometry's boundaries, so a `Solid` groups by shell and the 5-deep types
/// group by shell and then by solid.
///
/// The group sizes come from the *boundary* count arrays, because a semantics
/// mapping carries none of its own — one semantic value per surface is the
/// whole of its structure.
pub(crate) fn decode_semantics<V: UnflattenValues<SurfaceIndex>>(
    solids: &[u32],
    shells: &[u32],
    semantics_objects: &[SemanticObject],
    semantics_values: Option<&[u32]>,
    semantic_attr_schema: Option<flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<Column<'_>>>>,
) -> Semantics<V> {
    Semantics {
        surfaces: decode_semantics_surfaces(semantics_objects, semantic_attr_schema),
        // No values vector at all is `"values": null` -- a member whose value
        // is null, which the schema requires to be present and permits to be
        // null.
        values: semantics_values.map(|values| V::unflatten(solids, shells, values)),
        other: serde_json::Map::new(),
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

/// Rebuilds each theme's `material` in the shape `V` the geometry variant
/// names.
///
/// `material.values` is nested two levels less deeply than the boundaries, so
/// a `MultiSurface` gets one index per surface, a `Solid` one array per shell,
/// and a `MultiSolid`/`CompositeSolid` one array per shell per solid.
pub(crate) fn decode_materials<V: UnflattenValues<MaterialIndex>>(
    material_mappings: &[MaterialMapping],
) -> Option<Themes<CjMaterialReference<V>>> {
    if material_mappings.is_empty() {
        return None;
    }

    let materials = material_mappings
        .iter()
        .map(|mapping| {
            let theme = mapping.theme().unwrap_or("theme").to_string();
            let mut reference = CjMaterialReference {
                value: None,
                values: None,
                other: serde_json::Map::new(),
            };

            // A `value` colours the whole object and has no depth at all.
            if let Some(value) = mapping.value() {
                reference.value = Some(MaterialIndex(value as usize));
                return (theme, reference);
            }

            // No `vertices` vector at all is `"values": null` — an explicit
            // null, which the schema distinguishes from an absent `values`.
            reference.values = Some(mapping.vertices().map(|vertices| {
                let solids = mapping.solids().map(|s| s.iter().collect::<Vec<_>>());
                let shells = mapping.shells().map(|s| s.iter().collect::<Vec<_>>());
                V::unflatten(
                    solids.as_deref().unwrap_or_default(),
                    shells.as_deref().unwrap_or_default(),
                    &vertices.iter().collect::<Vec<_>>(),
                )
            }));
            (theme, reference)
        })
        .collect();

    Some(materials)
}

/// The three shapes `texture.values` comes in, each rebuilt level by level
/// from a [`TextureCursor`].
pub(crate) trait UnflattenTextures: Sized {
    fn unflatten(cursor: &mut TextureCursor<'_>) -> Result<Self, Error>;
}

impl UnflattenTextures for Vec<TexturedSurface> {
    /// Per surface, per ring.
    fn unflatten(cursor: &mut TextureCursor<'_>) -> Result<Self, Error> {
        (0..cursor.surfaces.len())
            .map(|_| cursor.take_surface())
            .collect()
    }
}

impl UnflattenTextures for Vec<TexturedShell> {
    /// ... per shell.
    fn unflatten(cursor: &mut TextureCursor<'_>) -> Result<Self, Error> {
        (0..cursor.shells.len())
            .map(|_| cursor.take_shell())
            .collect()
    }
}

impl UnflattenTextures for Vec<Vec<TexturedShell>> {
    /// ... per solid.
    fn unflatten(cursor: &mut TextureCursor<'_>) -> Result<Self, Error> {
        let solids = cursor.solids;
        solids
            .iter()
            .map(|&n| (0..n).map(|_| cursor.take_shell()).collect())
            .collect()
    }
}

/// Rebuilds each theme's `texture` in the shape `V` the geometry variant
/// names.
///
/// `texture.values` is nested exactly as deeply as the geometry's boundaries,
/// each ring becoming `[texture_index, uv_index, ...]` or `[null]`. Unlike a
/// material, only the leaf is nullable — the schema types every intermediate
/// level as a plain `"array"` — so nothing here decodes an intermediate
/// `null`. A ring that is neither spelling is an error.
pub(crate) fn decode_textures<V: UnflattenTextures>(
    texture_mappings: &[TextureMapping],
) -> Result<Option<Themes<CjTextureReference<V>>>, Error> {
    if texture_mappings.is_empty() {
        return Ok(None);
    }

    let mut textures = Themes::new();

    for mapping in texture_mappings {
        let theme = mapping.theme().unwrap_or("theme").to_string();

        // A theme may legally carry no `values` at all, and is then written
        // with no arrays — distinct from a theme whose `values` is empty.
        let Some(vertices) = mapping.vertices().map(|v| v.iter().collect::<Vec<_>>()) else {
            textures.insert(
                theme,
                CjTextureReference {
                    values: None,
                    other: serde_json::Map::new(),
                },
            );
            continue;
        };
        let collect = |v: Option<flatbuffers::Vector<'_, u32>>| {
            v.map(|s| s.iter().collect::<Vec<_>>()).unwrap_or_default()
        };
        let solids = collect(mapping.solids());
        let shells = collect(mapping.shells());
        let surfaces = collect(mapping.surfaces());
        let strings = collect(mapping.strings());

        let mut cursor = TextureCursor {
            solids: &solids,
            surfaces: &surfaces,
            shells: &shells,
            strings: &strings,
            vertices: &vertices,
            shell_cursor: 0,
            surface_cursor: 0,
            string_cursor: 0,
            vertex_cursor: 0,
        };

        textures.insert(
            theme,
            CjTextureReference {
                values: Some(V::unflatten(&mut cursor)?),
                other: serde_json::Map::new(),
            },
        );
    }

    Ok(Some(textures))
}

/// The texture equivalent of [`BoundaryCursor`]: the same count arrays, but
/// the leaf holds `[texture_index, uv_index, ...]` rather than vertex
/// indices, and is nullable.
pub(crate) struct TextureCursor<'a> {
    solids: &'a [u32],
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
    fn take_ring(&mut self) -> Result<TexturedRing, Error> {
        let size = self.strings.get(self.string_cursor).copied().unwrap_or(0) as usize;
        self.string_cursor += 1;
        let end = (self.vertex_cursor + size).min(self.vertices.len());
        let ring: Vec<Option<usize>> = self.vertices[self.vertex_cursor..end]
            .iter()
            .map(|&v| (v != NULL).then_some(v as usize))
            .collect();
        self.vertex_cursor = end;
        TexturedRing::try_from(ring).map_err(|source| Error::CjseqError { source })
    }

    fn take_surface(&mut self) -> Result<TexturedSurface, Error> {
        let rings = self.surfaces.get(self.surface_cursor).copied().unwrap_or(0);
        self.surface_cursor += 1;
        (0..rings).map(|_| self.take_ring()).collect()
    }

    fn take_shell(&mut self) -> Result<TexturedShell, Error> {
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
    /// through `decode_materials` into the shape `V`, returning the decoded
    /// `values` as JSON so expectations read as CityJSON.
    fn decode_material_values<V: UnflattenValues<MaterialIndex> + serde::Serialize>(
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
        let decoded = decode_materials::<V>(&[mapping]).expect("one theme");
        serde_json::to_value(&decoded["t"].values).expect("values serialize")
    }

    fn decode_texture_values<V: UnflattenTextures + serde::Serialize>(
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
        let decoded = decode_textures::<V>(&[mapping])
            .expect("valid rings")
            .expect("one theme");
        serde_json::to_value(&decoded["t"].values).expect("values serialize")
    }

    #[test]
    fn test_decode_boundaries() -> Result<()> {
        // MultiPoint
        assert_eq!(
            serde_json::to_value(decode_points(&[2, 44, 0, 7]))?,
            json!([2, 44, 0, 7])
        );

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
        let materials =
            decode_materials::<SolidValues<MaterialIndex>>(&[mapping]).expect("one theme");
        assert_eq!(materials["theme1"].value, Some(MaterialIndex(5)));
        assert!(materials["theme1"].values.is_none());

        // MultiSurface: one index per surface.
        assert_eq!(
            decode_material_values::<PrimitiveValues<MaterialIndex>>(&[], &[], &[0, 1, NULL, 2]),
            json!([0, 1, null, 2])
        );
        assert_eq!(
            decode_material_values::<PrimitiveValues<MaterialIndex>>(&[], &[], &[0, 1, NULL, 2]),
            json!([0, 1, null, 2])
        );

        // Solid: one array per shell.
        assert_eq!(
            decode_material_values::<SolidValues<MaterialIndex>>(
                &[2],
                &[3, 3],
                &[0, 1, NULL, 2, 3, 4]
            ),
            json!([[0, 1, null], [2, 3, 4]])
        );

        // CompositeSolid: one array per shell, per solid.
        assert_eq!(
            decode_material_values::<MultiSolidValues<MaterialIndex>>(
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
            decode_material_values::<SolidValues<MaterialIndex>>(&[2], &[2, NULL], &[0, 1]),
            json!([[0, 1], null])
        );
        assert_eq!(
            decode_material_values::<MultiSolidValues<MaterialIndex>>(&[1, NULL], &[2], &[0, 1]),
            json!([[[0, 1]], null])
        );
    }

    /// Each ring is `[texture, uv, ...]`, or `[null]` for an untextured one,
    /// regrouped at the depth of the shape `V`.
    #[test]
    fn test_decode_textures() {
        // MultiSurface: per surface, per ring.
        assert_eq!(
            decode_texture_values::<Vec<TexturedSurface>>(
                &[],
                &[3],
                &[1, 1, 1],
                &[4, 1, 4],
                &[0, 10, 20, 30, NULL, 2, 12, 22, 32]
            ),
            json!([[[0, 10, 20, 30]], [[null]], [[2, 12, 22, 32]]])
        );

        // Solid: ... per shell.
        assert_eq!(
            decode_texture_values::<Vec<TexturedShell>>(
                &[2],
                &[2, 1],
                &[1, 1, 1],
                &[4, 1, 4],
                &[0, 10, 20, 30, NULL, 3, 13, 23, 33]
            ),
            json!([[[[0, 10, 20, 30]], [[null]]], [[[3, 13, 23, 33]]]])
        );

        // CompositeSolid: ... per solid.
        assert_eq!(
            decode_texture_values::<Vec<Vec<TexturedShell>>>(
                &[2, 1],
                &[1, 1, 1],
                &[1, 1, 1],
                &[3, 1, 3],
                &[0, 10, 20, NULL, 4, 14, 24]
            ),
            json!([[[[[0, 10, 20]]], [[[null]]]], [[[[4, 14, 24]]]]])
        );
    }

    /// A ring that is neither `[null]` nor `[texture, uv, ...]` -- here a
    /// texture with a missing uv -- is reported, not passed on.
    #[test]
    fn a_malformed_texture_ring_is_an_error() {
        let mut fbb = FlatBufferBuilder::new();
        let theme = fbb.create_string("t");
        let shells = fbb.create_vector(&[1u32]);
        let surfaces = fbb.create_vector(&[1u32]);
        let strings = fbb.create_vector(&[3u32]);
        let vertices = fbb.create_vector(&[0u32, 10, NULL]);
        let mapping = FbTextureMapping::create(
            &mut fbb,
            &TextureMappingArgs {
                theme: Some(theme),
                shells: Some(shells),
                surfaces: Some(surfaces),
                strings: Some(strings),
                vertices: Some(vertices),
                ..Default::default()
            },
        );
        fbb.finish(mapping, None);
        let buf = fbb.finished_data().to_vec();
        let mapping = flatbuffers::root::<FbTextureMapping>(&buf).expect("valid mapping");
        assert!(matches!(
            decode_textures::<Vec<TexturedSurface>>(&[mapping]),
            Err(Error::CjseqError { .. })
        ));
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
