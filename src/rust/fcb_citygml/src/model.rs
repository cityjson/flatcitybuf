//! The intermediate model: a city object as CityGML describes it, before
//! anything is quantised.
//!
//! The CityGML module readers build this; the converter turns it into
//! CityJSON. Keeping the two apart means coordinates stay real-world `f64`
//! for the whole of the reading half — the transform cannot be computed
//! until the last coordinate in the file has been seen — and it gives the
//! module readers one shape to fill in rather than a CityJSON structure to
//! assemble incrementally.

use serde_json::{Map, Value};

use crate::gml::GmlGeometry;

/// One boundary surface's semantics: what the surface *is*, plus whatever
/// attributes were written on it.
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticSurface {
    /// The CityJSON semantic surface type, e.g. `"RoofSurface"`.
    pub stype: String,
    pub attributes: Map<String, Value>,
}

/// One geometry of a city object, at one level of detail.
#[derive(Debug, Clone, PartialEq)]
pub struct IntermediateGeometry {
    /// The LoD as CityJSON spells it — the digit of the CityGML property
    /// name, as a string, because CityJSON allows `"2.1"`-style values too.
    pub lod: String,
    pub geometry: GmlGeometry,
    /// The semantic surfaces this geometry's polygons point into with
    /// [`crate::gml::Polygon3::sem_idx`].
    pub surfaces: Vec<SemanticSurface>,
}

/// One city object, with its nested objects.
#[derive(Debug, Clone, PartialEq)]
pub struct IntermediateObject {
    /// The `gml:id`, or a generated stand-in when the source has none.
    pub id: String,
    pub co_type: cjseq::CityObjectType,
    pub attributes: Map<String, Value>,
    pub geometries: Vec<IntermediateGeometry>,
    /// Building parts, installations and the like: objects that belong to
    /// this one and share its CityJSON feature.
    pub children: Vec<IntermediateObject>,
}

impl IntermediateObject {
    /// An object of `co_type` identified by `id`, with nothing filled in yet.
    pub(crate) fn new(id: String, co_type: cjseq::CityObjectType) -> Self {
        Self {
            id,
            co_type,
            attributes: Map::new(),
            geometries: Vec::new(),
            children: Vec::new(),
        }
    }
}
