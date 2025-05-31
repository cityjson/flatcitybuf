use cjseq::conv::obj;
use cjseq::CityJSON;
use log::{debug, error};
use serde_wasm_bindgen::from_value;
use wasm_bindgen::prelude::*;

/// Converts a CityJSON object to OBJ format.
///
/// # Arguments
///
/// * `city_json_js` - JsValue containing a CityJSON object that will be deserialized
///   using serde_wasm_bindgen into the CityJSON struct defined by cjseq
///
/// # Returns
///
/// A string containing the OBJ data or an error
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_name = cjToObj)]
pub fn convert_cityjson_to_obj(city_json_js: &JsValue) -> Result<String, JsValue> {
    debug!("starting: convert_cityjson_to_obj");

    // Convert JsValue to CityJSON struct using serde_wasm_bindgen
    // This will first convert the JS object to a serde_json::Value and then
    // deserialize it into the CityJSON struct
    let city_json: CityJSON = match from_value(city_json_js.clone()) {
        Ok(json) => json,
        Err(e) => {
            error!("failed to deserialize CityJSON: {}", e);
            return Err(JsValue::from_str(&format!(
                "failed to parse cityjson: {}",
                e
            )));
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
