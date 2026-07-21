use crate::types::{
    value_to_python, CityJSON, CityObject, Feature, Geometry, Metadata, Transform, Vertex,
};
use cjseq::CityJSONFeature;
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Convert a CityJSONFeature from cjseq to Python Feature
pub fn cityjson_feature_to_python(py: Python, cj_feature: &CityJSONFeature) -> PyResult<Feature> {
    let id = cj_feature.id.clone();
    // The CityJSON type members are enums now; Python wants their spelling.
    let feature_type = json_name(serde_json::to_value(&cj_feature.thetype));

    // Convert vertices from Vec<Vec<i64>> to Vec<Vertex>
    let vertices: Vec<Vertex> = cj_feature
        .vertices
        .iter()
        .map(|v| {
            let x = v.get(0).unwrap_or(&0) as &i64;
            let y = v.get(1).unwrap_or(&0) as &i64;
            let z = v.get(2).unwrap_or(&0) as &i64;
            Vertex::new(*x as f64, *y as f64, *z as f64)
        })
        .collect();

    // Convert CityObjects to Python dictionary
    let city_objects_dict = PyDict::new(py);
    for (obj_id, city_obj) in &cj_feature.city_objects {
        let py_city_obj = cityjson_cityobject_to_python(py, city_obj)?;
        city_objects_dict.set_item(obj_id, Py::new(py, py_city_obj)?)?;
    }

    Ok(Feature::new(
        id,
        feature_type,
        city_objects_dict.to_object(py),
        vertices,
    ))
}

/// Convert FlatCityBuf CityFeature to CityJSONFeature, then to Python
/// This is a bridge function that uses the fcb_core deserialization
pub fn cityfeature_to_python(
    py: Python,
    buffer: &fcb_core::city_buffer::FcbBuffer,
) -> PyResult<Feature> {
    // Use fcb_core to deserialize to CityJSONFeature
    let cj_feature = buffer.cj_feature().map_err(|e| {
        PyErr::new::<crate::error::FcbError, _>(format!("Failed to deserialize CityJSON: {}", e))
    })?;

    // Convert the CityJSONFeature to Python
    cityjson_feature_to_python(py, &cj_feature)
}

/// Convert a CityJSON CityObject to Python CityObject
fn cityjson_cityobject_to_python(py: Python, city_obj: &cjseq::CityObject) -> PyResult<CityObject> {
    let obj_type = json_name(serde_json::to_value(&city_obj.thetype));

    // Convert geometries
    let mut geometries = Vec::new();
    if let Some(geom_vec) = &city_obj.geometry {
        for geom in geom_vec {
            geometries.push(cityjson_geometry_to_python(py, geom)?);
        }
    }

    // Convert attributes
    let attributes_dict = PyDict::new(py);
    if let Some(attrs) = &city_obj.attributes {
        for (key, value) in attrs.as_object().unwrap_or(&serde_json::Map::new()) {
            attributes_dict.set_item(key, value_to_python(py, value)?)?;
        }
    }

    // Convert children and parents
    let children = city_obj.children.clone();
    let parents = city_obj.parents.clone();

    Ok(CityObject::new(
        obj_type,
        geometries,
        attributes_dict.to_object(py),
        children,
        parents,
    ))
}

/// Convert a CityJSON Geometry to Python Geometry
fn cityjson_geometry_to_python(py: Python, geom: &cjseq::Geometry) -> PyResult<Geometry> {
    let geom_type = json_name(serde_json::to_value(geom.geometry_type()));

    // The nesting depth of `boundaries` is part of the geometry's type, so it
    // is serialized rather than walked: `serde` already knows the depth.
    let boundaries = convert_boundaries_to_python(py, geom)?;

    // Convert semantics if available
    let semantics = if let Some(sem) = geom.common().and_then(|c| c.semantics.as_ref()) {
        let sem_dict = PyDict::new(py);
        // Convert semantics structure to Python dict
        if let Ok(sem_value) = serde_json::to_value(sem) {
            for (key, value) in sem_value.as_object().unwrap_or(&serde_json::Map::new()) {
                sem_dict.set_item(key, value_to_python(py, value)?)?;
            }
        }
        Some(sem_dict.to_object(py))
    } else {
        None
    };

    Ok(Geometry::new(
        geom_type,
        Vec::new(), // Vertices are at feature level in CityJSON
        boundaries,
        semantics,
    ))
}

/// Convert FCB header to CityJSON Python object
/// This creates a proper CityJSON object with transform and metadata
pub fn header_to_cityjson(_py: Python, header: &fcb_core::Header) -> PyResult<CityJSON> {
    // Extract transform information
    let transform = if let Some(fcb_transform) = header.transform() {
        Transform::new(
            vec![
                fcb_transform.scale().x(),
                fcb_transform.scale().y(),
                fcb_transform.scale().z(),
            ],
            vec![
                fcb_transform.translate().x(),
                fcb_transform.translate().y(),
                fcb_transform.translate().z(),
            ],
        )
    } else {
        // Default CityJSON transform
        Transform::new(vec![1.0, 1.0, 1.0], vec![0.0, 0.0, 0.0])
    };

    // Extract metadata information
    let metadata = {
        let mut has_metadata = false;
        let geographical_extent = header.geographical_extent().map(|extent| {
            has_metadata = true;
            vec![
                extent.min().x(),
                extent.min().y(),
                extent.min().z(),
                extent.max().x(),
                extent.max().y(),
                extent.max().z(),
            ]
        });

        let identifier = header.identifier().map(|id| {
            has_metadata = true;
            id.to_string()
        });

        let title = header.title().map(|t| {
            has_metadata = true;
            t.to_string()
        });

        let reference_system = header.reference_system().map(|crs| {
            has_metadata = true;
            if let Some(auth) = crs.authority() {
                format!("{}:{}", auth, crs.code())
            } else {
                format!("EPSG:{}", crs.code())
            }
        });

        if has_metadata {
            Some(Metadata::new(
                geographical_extent,
                identifier,
                None, // reference_date not available in FCB header
                reference_system,
                title,
            ))
        } else {
            None
        }
    };

    // Create CityJSON object
    Ok(CityJSON::new(
        "CityJSON".to_string(),
        header.version().to_string(),
        transform,
        header.features_count(),
        metadata,
    ))
}

/// Check if a string is a URL
pub fn is_url(path: &str) -> bool {
    path.starts_with("http://") || path.starts_with("https://")
}

/// The CityJSON spelling of a type tag. Each is an enum that serializes to its
/// bare name (or, for an Extension, to its `+`-prefixed name verbatim), so the
/// serialization is the name -- unlike `{:?}`, which would print
/// `Known(Building)`.
fn json_name(value: serde_json::Result<serde_json::Value>) -> String {
    match value {
        Ok(serde_json::Value::String(s)) => s,
        other => format!("{other:?}"),
    }
}

/// Converts a geometry's `boundaries` to nested Python lists.
///
/// The depth is part of the geometry's type, so this serializes the geometry
/// and reads `boundaries` back out rather than walking an untyped tree and
/// inferring the depth from what it finds.
fn convert_boundaries_to_python(py: Python, geom: &cjseq::Geometry) -> PyResult<PyObject> {
    let value = serde_json::to_value(geom).map_err(|e| {
        PyErr::new::<crate::error::FcbError, _>(format!("Failed to serialize geometry: {e}"))
    })?;
    let boundaries = value
        .get("boundaries")
        .cloned()
        .unwrap_or(serde_json::Value::Array(Vec::new()));
    value_to_python(py, &boundaries)
}
