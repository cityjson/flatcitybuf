use crate::types::*;
// use cjseq::CityJSONFeature; // TODO: Fix cjseq dependency
use fcb_core::fb::{CityFeature, CityObject, Vertex as FbVertex};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::Value;

/// Convert a FlatCityBuf CityFeature to Python Feature
pub fn cityfeature_to_python(py: Python, feature: CityFeature) -> PyResult<Feature> {
    let id = feature.id().map(|s| s.to_string());
    
    // Extract vertices
    let vertices = if let Some(vertex_vec) = feature.vertices() {
        vertex_vec
            .iter()
            .map(|v| Vertex::new(v.x() as f64, v.y() as f64, v.z() as f64))
            .collect()
    } else {
        Vec::new()
    };

    // Extract geometries
    let geometries = if let Some(objects) = feature.objects() {
        let mut geoms = Vec::new();
        for obj in objects.iter() {
            if let Some(geometry) = extract_geometry_from_object(py, &obj, &vertices)? {
                geoms.push(geometry);
            }
        }
        geoms
    } else {
        Vec::new()
    };

    // For now, use the first object's type if available
    let feature_type = if let Some(objects) = feature.objects() {
        if objects.len() > 0 {
            let first_obj = objects.get(0);
            format!("{:?}", first_obj.type_())  // This will need proper conversion
        } else {
            "Unknown".to_string()
        }
    } else {
        "Unknown".to_string()
    };

    // Extract attributes (simplified for now)
    let attributes = PyDict::new(py).to_object(py);

    Ok(Feature::new(feature_type, id, geometries, Some(attributes)))
}

/// Convert a CityJSONFeature to Python Feature (placeholder)
pub fn cityjson_to_python(py: Python, _feature: serde_json::Value) -> PyResult<Feature> {
    // TODO: Implement proper CityJSON conversion
    let id = Some("placeholder".to_string());
    let feature_type = "Unknown".to_string();
    let geometries = Vec::new();
    
    // Placeholder attributes
    let attributes = py.None();
    
    Ok(Feature::new(feature_type, id, geometries, Some(attributes)))
}

fn extract_geometry_from_object(
    _py: Python,
    _obj: &CityObject,
    _vertices: &[Vertex],
) -> PyResult<Option<Geometry>> {
    // TODO: Implement proper geometry extraction from FlatBuffers CityObject
    // This is a placeholder that would need to handle the FlatBuffers geometry structure
    Ok(Some(Geometry::new(
        "Unknown".to_string(),
        Vec::new(),
        Vec::new(),
        None,
    )))
}

/// Check if a string is a URL
pub fn is_url(path: &str) -> bool {
    path.starts_with("http://") || path.starts_with("https://")
}

/// Convert FlatBuffers vertex to Python vertex with scale and translation
pub fn fb_vertex_to_python(vertex: &FbVertex, scale: (f64, f64, f64), translate: (f64, f64, f64)) -> Vertex {
    Vertex::new(
        vertex.x() as f64 * scale.0 + translate.0,
        vertex.y() as f64 * scale.1 + translate.1,
        vertex.z() as f64 * scale.2 + translate.2,
    )
}