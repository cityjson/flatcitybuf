use crate::types::{js_value_to_keytype, parse_operator};

use fcb_core::packed_rtree::Query as SpatialQuery;
use fcb_core::AttrQuery;
use napi::bindgen_prelude::*;
use napi_derive::napi;

/// Spatial query for filtering features by location.
///
/// Supports three query types:
/// - `bbox`: Bounding box query with minX, minY, maxX, maxY
/// - `pointIntersects`: Point intersection query with x, y
/// - `pointNearest`: Nearest point query with x, y
#[napi]
pub struct NodeSpatialQuery {
    inner: SpatialQuery,
}

#[napi]
impl NodeSpatialQuery {
    /// Create a bounding box query.
    #[napi(factory)]
    pub fn bbox(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> NodeSpatialQuery {
        NodeSpatialQuery {
            inner: SpatialQuery::BBox(min_x, min_y, max_x, max_y),
        }
    }

    /// Create a point intersection query.
    #[napi(factory)]
    pub fn point_intersects(x: f64, y: f64) -> NodeSpatialQuery {
        NodeSpatialQuery {
            inner: SpatialQuery::PointIntersects(x, y),
        }
    }

    /// Create a nearest point query.
    #[napi(factory)]
    pub fn point_nearest(x: f64, y: f64) -> NodeSpatialQuery {
        NodeSpatialQuery {
            inner: SpatialQuery::PointNearest(x, y),
        }
    }

    /// Get the query type as a string.
    #[napi(getter)]
    pub fn query_type(&self) -> String {
        match self.inner {
            SpatialQuery::BBox(_, _, _, _) => "bbox".to_string(),
            SpatialQuery::PointIntersects(_, _) => "pointIntersects".to_string(),
            SpatialQuery::PointNearest(_, _) => "pointNearest".to_string(),
        }
    }
}

impl NodeSpatialQuery {
    pub fn to_core_query(&self) -> Result<SpatialQuery> {
        Ok(match self.inner {
            SpatialQuery::BBox(a, b, c, d) => SpatialQuery::BBox(a, b, c, d),
            SpatialQuery::PointIntersects(x, y) => SpatialQuery::PointIntersects(x, y),
            SpatialQuery::PointNearest(x, y) => SpatialQuery::PointNearest(x, y),
        })
    }
}

/// Attribute query for filtering features by attribute values.
///
/// Constructed from an array of conditions, where each condition
/// is a tuple of [field, operator, value].
///
/// Operators: "Eq", "Gt", "Ge", "Lt", "Le", "Ne"
///
/// Example:
/// ```js
/// const query = new NodeAttrQuery([
///   ["height", "Gt", 10.0],
///   ["name", "Eq", "building-1"]
/// ]);
/// ```
#[napi]
pub struct NodeAttrQuery {
    inner: AttrQuery,
}

#[napi]
impl NodeAttrQuery {
    /// Create an attribute query from an array of condition tuples.
    ///
    /// Each condition is [field: string, operator: string, value: any].
    #[napi(constructor)]
    pub fn new(conditions: Vec<serde_json::Value>) -> Result<NodeAttrQuery> {
        let mut inner: AttrQuery = Vec::new();

        for condition in conditions {
            let arr = condition
                .as_array()
                .ok_or_else(|| Error::from_reason("Each condition must be an array"))?;
            if arr.len() < 3 {
                return Err(Error::from_reason(
                    "Each condition must have 3 elements: [field, operator, value]",
                ));
            }

            let field = arr[0]
                .as_str()
                .ok_or_else(|| Error::from_reason("Field must be a string"))?
                .to_string();

            let op_str = arr[1]
                .as_str()
                .ok_or_else(|| Error::from_reason("Operator must be a string"))?;
            let operator = parse_operator(op_str)?;

            let value = js_value_to_keytype(&arr[2])?;

            inner.push((field, operator, value));
        }

        Ok(NodeAttrQuery { inner })
    }
}

impl NodeAttrQuery {
    pub fn to_core_query(&self) -> Result<AttrQuery> {
        Ok(self.inner.clone())
    }
}
