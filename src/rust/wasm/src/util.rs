use cjseq::conv::obj;
use cjseq::{CityJSON, CityJSONFeature};
use log::{debug, error};
use serde_wasm_bindgen::from_value;
use serde_wasm_bindgen::to_value;
use wasm_bindgen::prelude::*;

/// Converts a list of CityJSONFeature objects into a single CityJSON object.
///
/// # Arguments
///
/// * `features` - Vector of CityJSONFeature objects to merge
/// * `base_cj` - Base CityJSON object to merge features into
///
/// # Returns
///
/// A single CityJSON object with all features merged
pub fn cjseq_to_cj(mut base_cj: CityJSON, features: Vec<CityJSONFeature>) -> CityJSON {
    debug!("starting: cjseq_to_cj with {} features", features.len());

    for mut feature in features {
        base_cj.add_cjfeature(&mut feature);
    }

    // Process like collect_from_file
    base_cj.remove_duplicate_vertices();
    base_cj.update_transform();

    debug!("completed: cjseq_to_cj");
    base_cj
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_name = cjseqToCj)]
pub fn cjseq_to_cj_wasm(base_cj: JsValue, features: JsValue) -> Result<JsValue, JsValue> {
    let base_cj: CityJSON = match from_value(base_cj) {
        Ok(cj) => cj,
        Err(e) => {
            error!("failed to deserialize base_cj: {}", e);
            return Err(JsValue::from_str(&format!(
                "failed to parse base_cj: {}",
                e
            )));
        }
    };

    let features: Vec<CityJSONFeature> = match from_value(features) {
        Ok(f) => f,
        Err(e) => {
            error!("failed to deserialize features: {}", e);
            return Err(JsValue::from_str(&format!(
                "failed to parse features: {}",
                e
            )));
        }
    };

    let cj = cjseq_to_cj(base_cj, features);

    match to_value(&cj) {
        Ok(js_val) => Ok(js_val),
        Err(e) => {
            error!("failed to serialize cj: {}", e);
            return Err(JsValue::from_str(&format!("failed to serialize cj: {}", e)));
        }
    }
}

/// Converts a CityJSON object or CityJSONSeq list to OBJ format.
///
/// # Arguments
///
/// * `city_json_js` - JsValue containing either:
///   - A CityJSON object (for backward compatibility), or
///   - An array where the first element is a CityJSON object and
///     the rest are CityJSONFeature objects (CityJSONSeq format)
///
/// # Returns
///
/// A string containing the OBJ data or an error
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_name = cjToObj)]
pub fn convert_cityjson_to_obj(city_json_js: &JsValue) -> Result<String, JsValue> {
    debug!("starting: convert_cityjson_to_obj");

    // Check if input is an array (CityJSONSeq format) or single object (CityJSON format)
    let city_json: CityJSON = if js_sys::Array::is_array(city_json_js) {
        // Handle CityJSONSeq format: array with CityJSON first, then CityJSONFeature objects
        let array = js_sys::Array::from(city_json_js);
        let length = array.length();

        if length == 0 {
            return Err(JsValue::from_str("empty array provided"));
        }

        // First element should be CityJSON
        let first_element = array.get(0);
        let mut cj: CityJSON = match from_value(first_element) {
            Ok(json) => json,
            Err(e) => {
                error!("failed to deserialize first element as CityJSON: {}", e);
                return Err(JsValue::from_str(&format!(
                    "failed to parse first element as cityjson: {}",
                    e
                )));
            }
        };

        // Remaining elements should be CityJSONFeature objects
        let mut features = Vec::new();
        for i in 1..length {
            let element = array.get(i);
            let feature: CityJSONFeature = match from_value(element) {
                Ok(feature) => feature,
                Err(e) => {
                    error!(
                        "failed to deserialize element {} as CityJSONFeature: {}",
                        i, e
                    );
                    return Err(JsValue::from_str(&format!(
                        "failed to parse element {} as cityjsonfeature: {}",
                        i, e
                    )));
                }
            };
            features.push(feature);
        }

        debug!("processing cityjsonseq with {} features", features.len());

        // Merge all features into the base CityJSON object
        cjseq_to_cj(cj, features)
    } else {
        // Handle single CityJSON object (backward compatibility)
        match from_value(city_json_js.clone()) {
            Ok(json) => json,
            Err(e) => {
                error!("failed to deserialize CityJSON: {}", e);
                return Err(JsValue::from_str(&format!(
                    "failed to parse cityjson: {}",
                    e
                )));
            }
        }
    };

    // Pass the parsed CityJSON to the obj conversion function
    let obj_string = obj::to_obj_string(&city_json);

    debug!(
        "completed: convert_cityjson_to_obj, generated OBJ with {} bytes",
        obj_string.len()
    );
    Ok(obj_string)
}
