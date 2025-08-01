mod test_data;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use test_data::{
    EXPECTED_COLLECTIONS, EXPECTED_COLLECTION_PAND, EXPECTED_CONFORMANCE, EXPECTED_LANDING_PAGE,
};
use tower::util::ServiceExt;

async fn app() -> axum::Router {
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
                .uri(&format!("/collections/pand/items?bbox={}&limit=5", bbox))
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
    println!("features: {:?}", features);
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

    // Use a known test ID or skip if not available
    let test_id = "NL.IMBAG.Pand.0851100000000564";

    let response = app
        .oneshot(
            Request::builder()
                .uri(&format!("/collections/pand/items/{}", test_id))
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
        println!("json: {:?}", json);
    }
}
