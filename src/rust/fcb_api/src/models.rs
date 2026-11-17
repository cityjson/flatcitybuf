pub use openapi::models::feature_collection::Type;
pub use openapi::models::*;

#[derive(serde::Serialize)]
pub struct FeatureCollection {
    #[serde(rename = "type")]
    pub r#type: Type,
    pub metadata: serde_json::Value,
    pub features: Vec<serde_json::Value>,
    pub links: Option<Vec<Link>>,
    #[serde(rename = "numberMatched")]
    pub number_matched: Option<u64>,
    #[serde(rename = "numberReturned")]
    pub number_returned: Option<u64>,
    #[serde(rename = "timeStamp")]
    pub time_stamp: Option<String>,
}

#[derive(serde::Serialize)]
pub struct FeatureCityJson {
    pub id: Option<String>,
    pub metadata: serde_json::Value,
    pub feature: Option<serde_json::Value>,
    pub links: Option<Vec<Link>>,
}
