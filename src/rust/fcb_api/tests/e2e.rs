mod test_data;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use cjseq::CityJSONFeature;
use openapi::models::FeatureCollection;
use serde_json::Value;
use test_data::{
    EXPECTED_COLLECTIONS, EXPECTED_COLLECTION_PAND, EXPECTED_CONFORMANCE, EXPECTED_LANDING_PAGE,
};
use tower::util::ServiceExt;

async fn app() -> axum::Router {
    std::env::set_var("BASE_URL", "https://api.3dbag.nl");
    fcb_api::create_app().await
}

#[tokio::test]
async fn test_landing_page() {
    let app = app().await;

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let expected_json: Value = serde_json::from_str(EXPECTED_LANDING_PAGE).unwrap();
    assert_eq!(json, expected_json);
}

#[tokio::test]
async fn test_conformance() {
    let app = app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/conformance")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let expected_json: Value = serde_json::from_str(EXPECTED_CONFORMANCE).unwrap();
    assert_eq!(json, expected_json);
}

#[tokio::test]
async fn test_collections() {
    let app = app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/collections")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let expected_json: Value = serde_json::from_str(EXPECTED_COLLECTIONS).unwrap();
    assert_eq!(json, expected_json);
}

#[tokio::test]
async fn test_collection_by_id() {
    let app = app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/collections/pand")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let expected_json: Value = serde_json::from_str(EXPECTED_COLLECTION_PAND).unwrap();
    assert_eq!(json, expected_json);
}

#[tokio::test]
async fn test_collection_not_found() {
    let app = app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/collections/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_collection_items_with_limit() {
    let app = app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/collections/pand/items?limit=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["type"], "FeatureCollection");
    assert!(json["features"].is_array());

    let features = json["features"].as_array().unwrap();
    assert!(features.len() <= 2);

    // Check feature structure
    if !features.is_empty() {
        assert!(features[0]["id"].is_string());
        assert!(features[0]["feature"].is_object());
        assert!(features[0]["links"].is_array());
    }
}

#[tokio::test]
async fn test_collection_items_with_bbox() {
    let app = app().await;

    let bbox = "68989.19384501831,444614.3991728433,70685.16687543111,446023.6031208569";
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/collections/pand/items?bbox={bbox}&limit=5"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["type"], "FeatureCollection");

    let features = json["features"].as_array().unwrap();
    assert!(features.len() <= 5);
}

#[tokio::test]
async fn test_invalid_bbox_format() {
    let app = app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/collections/pand/items?bbox=invalid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_bbox_with_wrong_number_of_coords() {
    let app = app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/collections/pand/items?bbox=1,2,3")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_collection_item_by_id() {
    let app = app().await;

    let test_id = "NL.IMBAG.Pand.0851100000000564";

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/collections/pand/items/{test_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    if response.status() == StatusCode::OK {
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["id"], test_id);
        assert!(json["feature"].is_object());
        assert!(json["links"].is_array());
    }
}

#[tokio::test]
async fn test_filter_simple_equality() {
    let app = app().await;

    let test_id = "NL.IMBAG.Pand.0503100000012869";
    let filter = format!("identificatie = '{test_id}'");
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/collections/pand/items?filter={}&limit=5",
                    urlencoding::encode(&filter)
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json["features"].is_array());
    assert!(json["features"].as_array().unwrap().len() == 1);
    assert!(
        json["features"].as_array().unwrap()[0]["id"]
            .as_str()
            .unwrap()
            == test_id
    );
}

#[tokio::test]
async fn test_filter_numeric_comparison() {
    let app = app().await;

    let filter = "b3_h_dak_50p > 100";
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/collections/pand/items?filter={}&limit=10",
                    urlencoding::encode(filter)
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    if response.status() == StatusCode::OK {
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: FeatureCollection = serde_json::from_slice(&body).unwrap();
        let features = json.features;
        for feature in features {
            let mut found = false;
            let feature: CityJSONFeature =
                serde_json::from_value(feature.feature.unwrap()).unwrap();
            for co in feature.city_objects.values() {
                if let Some(attrs) = co.attributes.as_ref() {
                    if let Some(b3_h_dak_50p) = attrs.get("b3_h_dak_50p") {
                        if b3_h_dak_50p.as_f64().unwrap() > 10.0 {
                            found = true;
                        }
                    }
                }
            }
            assert!(found);
        }
    }
}

#[tokio::test]
async fn test_filter_and_condition() {
    let app = app().await;

    let filter = "b3_h_dak_50p > 10 AND b3_bouwlagen >= 2";
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/collections/pand/items?filter={}&limit=3",
                    urlencoding::encode(filter)
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Handle case where attributes might not be indexed in test dataset
    assert!(
        response.status() == StatusCode::OK,
        "Expected 200 got: {}",
        response.status()
    );

    if response.status() == StatusCode::OK {
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: FeatureCollection = serde_json::from_slice(&body).unwrap();
        let features = json.features;

        // Verify that all returned features match both conditions:
        // b3_h_dak_50p > 10 AND b3_bouwlagen >= 2
        for feature in features {
            let mut height_condition_met = false;
            let mut floors_condition_met = false;

            let feature: CityJSONFeature =
                serde_json::from_value(feature.feature.unwrap()).unwrap();
            for co in feature.city_objects.values() {
                if let Some(attrs) = co.attributes.as_ref() {
                    // Check b3_h_dak_50p > 10
                    if let Some(height) = attrs.get("b3_h_dak_50p") {
                        if height.as_f64().unwrap() > 10.0 {
                            height_condition_met = true;
                        }
                    }
                    // Check b3_bouwlagen >= 2
                    if let Some(floors) = attrs.get("b3_bouwlagen") {
                        if floors.as_i64().unwrap() >= 2 {
                            floors_condition_met = true;
                        }
                    }
                }
            }
            // Both conditions must be met
            assert!(height_condition_met, "Height condition not met for feature");
            assert!(floors_condition_met, "Floors condition not met for feature");
        }
    }
}

// TODO: add test for BETWEEN condition
// #[tokio::test]
// async fn test_filter_between_condition() {
//     let app = app().await;

//     let filter = "b3_h_dak_50p BETWEEN 5.0 AND 20.0";
//     let response = app
//         .oneshot(
//             Request::builder()
//                 .uri(format!(
//                     "/collections/pand/items?filter={}&limit=3",
//                     urlencoding::encode(filter)
//                 ))
//                 .body(Body::empty())
//                 .unwrap(),
//         )
//         .await
//         .unwrap();

//     // Handle case where attributes might not be indexed in test dataset
//     assert!(
//         response.status() == StatusCode::OK,
//         "Expected 200, got: {}",
//         response.status()
//     );

//     if response.status() == StatusCode::OK {
//         let body = axum::body::to_bytes(response.into_body(), usize::MAX)
//             .await
//             .unwrap();
//         let json: FeatureCollection = serde_json::from_slice(&body).unwrap();
//         let features = json.features;

//         // Verify that all returned features match BETWEEN condition:
//         // b3_h_dak_50p BETWEEN 5 AND 20 (inclusive)
//         for feature in features {
//             let mut between_condition_met = false;

//             let feature: CityJSONFeature =
//                 serde_json::from_value(feature.feature.unwrap()).unwrap();
//             for co in feature.city_objects.values() {
//                 if let Some(attrs) = co.attributes.as_ref() {
//                     // Check b3_h_dak_50p >= 5 AND b3_h_dak_50p <= 20
//                     if let Some(height) = attrs.get("b3_h_dak_50p") {
//                         let height_value = height.as_f64().unwrap();
//                         if (5.0..=20.0).contains(&height_value) {
//                             between_condition_met = true;
//                         }
//                     }
//                 }
//             }
//             assert!(
//                 between_condition_met,
//                 "BETWEEN condition not met for feature"
//             );
//         }
//     }
// }

#[tokio::test]
async fn test_filter_boolean_value() {
    let app = app().await;

    let filter = "b3_is_glas_dak = true";
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/collections/pand/items?filter={}&limit=3",
                    urlencoding::encode(filter)
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Handle case where attributes might not be indexed in test dataset
    assert!(
        response.status() == StatusCode::OK,
        "Expected 200, got: {}",
        response.status()
    );

    if response.status() == StatusCode::OK {
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: FeatureCollection = serde_json::from_slice(&body).unwrap();
        let features = json.features;

        // Verify that all returned features match boolean condition:
        // b3_is_glas_dak = true
        for feature in features {
            let mut boolean_condition_met = false;

            let feature: CityJSONFeature =
                serde_json::from_value(feature.feature.unwrap()).unwrap();
            for co in feature.city_objects.values() {
                if let Some(attrs) = co.attributes.as_ref() {
                    // Check b3_is_glas_dak = true
                    if let Some(is_glass_roof) = attrs.get("b3_is_glas_dak") {
                        if is_glass_roof.as_bool().unwrap_or(false) {
                            boolean_condition_met = true;
                        }
                    }
                }
            }
            assert!(
                boolean_condition_met,
                "Boolean condition not met for feature"
            );
        }
    }
}

#[tokio::test]
async fn test_filter_invalid_syntax() {
    let app = app().await;

    let filter = "building_height >> 30";
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/collections/pand/items?filter={}",
                    urlencoding::encode(filter)
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_filter_combined_with_bbox() {
    let app = app().await;

    let bbox = "68989.19384501831,444614.3991728433,70685.16687543111,446023.6031208569";
    let filter = "b3_h_dak_50p > 5";
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/collections/pand/items?bbox={}&filter={}&limit=3",
                    bbox,
                    urlencoding::encode(filter)
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Handle case where attributes might not be indexed in test dataset
    assert!(
        response.status() == StatusCode::OK,
        "Expected 200, got: {}",
        response.status()
    );

    if response.status() == StatusCode::OK {
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["type"], "FeatureCollection");
        assert!(json["features"].is_array());
        let features = json["features"].as_array().unwrap();
        assert!(features.len() <= 3);
    }
}
