use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use fcb_api::{create_app_with_config, AppConfig};
use fcb_core::{
    attribute::{AttributeSchema, AttributeSchemaMethods},
    header_writer::HeaderWriterOptions,
    read_cityjson_from_reader, CJType, CJTypeKind, FcbReader, FcbWriter,
};
use serde_json::{json, Value};
use std::{
    io::{BufReader, Cursor},
    sync::{
        atomic::{AtomicBool, AtomicU16, Ordering},
        Arc,
    },
};
use tower::ServiceExt;

struct Dataset {
    file: tempfile::NamedTempFile,
    metadata: Value,
    features: Vec<Value>,
}
impl Dataset {
    fn new() -> Self {
        Self::with_count(3)
    }
    fn with_count(count: usize) -> Self {
        let header = json!({"type":"CityJSON","version":"2.0","transform":{"scale":[0.01,0.02,0.03],"translate":[85000.25,446000.5,3.0]},"CityObjects":{},"vertices":[],"metadata":{"referenceSystem":"https://www.opengis.net/def/crs/EPSG/0/7415","title":"API fixture"}});
        let mut input = format!("{header}\n");
        for i in 0..count {
            let id = format!("NL.IMBAG.Pand.{i:016}");
            let f = json!({"type":"CityJSONFeature","id":id,"CityObjects":{id.clone():{"type":"Building","attributes":{"identificatie":id,"height":i as f64 + 1.0},"geometry":[{"type":"MultiPoint","lod":"1","boundaries":[0]}]}},"vertices":[[i*10,i*10,0]]});
            input.push_str(&format!("{f}\n"));
        }
        let CJType::Seq(seq) =
            read_cityjson_from_reader(BufReader::new(Cursor::new(input)), CJTypeKind::Seq).unwrap()
        else {
            panic!()
        };
        let mut schema = AttributeSchema::new();
        for f in &seq.features {
            for co in f.city_objects.values() {
                schema.add_attributes(co.attributes.as_ref().unwrap());
            }
        }
        let mut writer = FcbWriter::new(
            seq.cj,
            Some(HeaderWriterOptions {
                write_index: true,
                feature_count: count as u64,
                index_node_size: 16,
                attribute_indices: Some(vec![
                    ("identificatie".into(), None),
                    ("height".into(), None),
                ]),
                geographical_extent: Some([85000.25, 446000.5, 3.0, 85000.45, 446000.9, 3.0]),
            }),
            Some(schema),
            None,
        )
        .unwrap();
        for f in &seq.features {
            writer.add_feature(f).unwrap();
        }
        let mut file = tempfile::NamedTempFile::new().unwrap();
        writer.write(file.as_file_mut()).unwrap();
        let mut reader = FcbReader::open(BufReader::new(std::fs::File::open(file.path()).unwrap()))
            .unwrap()
            .select_all()
            .unwrap();
        let metadata =
            serde_json::to_value(fcb_core::deserializer::to_cj_metadata(&reader.header()).unwrap())
                .unwrap();
        let mut features = vec![];
        while let Some(f) = reader.next().unwrap() {
            features.push(serde_json::to_value(f.cur_cj_feature().unwrap()).unwrap());
        }
        Self {
            file,
            metadata,
            features,
        }
    }
    fn config(&self, url: String) -> AppConfig {
        AppConfig {
            fcb_url: url,
            base_url: "http://api.test".into(),
            max_return_features: 100,
            probe_timeout_seconds: 10,
            collection_version: None,
        }
    }
}
struct Server {
    url: String,
    unavailable: Arc<AtomicBool>,
    failure_status: Arc<AtomicU16>,
    delay: Arc<AtomicBool>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
type RangeState = (
    Arc<Vec<u8>>,
    Arc<AtomicBool>,
    Arc<AtomicU16>,
    Arc<AtomicBool>,
);
async fn serve(data: Vec<u8>) -> Server {
    use axum::{
        extract::State,
        http::{header, HeaderMap},
        response::IntoResponse,
        routing::get,
    };
    async fn range(
        State((data, down, status, delay)): State<RangeState>,
        headers: HeaderMap,
    ) -> axum::response::Response {
        if delay.load(Ordering::SeqCst) {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        }
        if down.load(Ordering::SeqCst) {
            return StatusCode::from_u16(status.load(Ordering::SeqCst))
                .unwrap()
                .into_response();
        }
        let range = headers
            .get(header::RANGE)
            .unwrap()
            .to_str()
            .unwrap()
            .strip_prefix("bytes=")
            .unwrap();
        let (a, b) = range.split_once('-').unwrap();
        let a: usize = a.parse().unwrap();
        if a >= data.len() {
            return StatusCode::RANGE_NOT_SATISFIABLE.into_response();
        }
        let b = b
            .parse::<usize>()
            .unwrap_or(data.len() - 1)
            .min(data.len() - 1);
        (
            StatusCode::PARTIAL_CONTENT,
            [(
                header::CONTENT_RANGE,
                format!("bytes {a}-{b}/{}", data.len()),
            )],
            data[a..=b].to_vec(),
        )
            .into_response()
    }
    let unavailable = Arc::new(AtomicBool::new(false));
    let failure_status = Arc::new(AtomicU16::new(503));
    let delay = Arc::new(AtomicBool::new(false));
    let app = Router::new().route("/data", get(range)).with_state((
        Arc::new(data),
        unavailable.clone(),
        failure_status.clone(),
        delay.clone(),
    ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/data", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    Server {
        url,
        unavailable,
        failure_status,
        delay,
        task,
    }
}
async fn request(app: &Router, uri: &str) -> (StatusCode, axum::http::HeaderMap, Value) {
    let r = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = r.status();
    let headers = r.headers().clone();
    let body = axum::body::to_bytes(r.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        headers,
        serde_json::from_slice(&body).unwrap_or(Value::Null),
    )
}
#[tokio::test]
async fn local_and_http_listing() {
    let d = Dataset::new();
    let server = serve(std::fs::read(d.file.path()).unwrap()).await;
    for url in [d.file.path().to_str().unwrap().into(), server.url.clone()] {
        let app = create_app_with_config(d.config(url)).await.unwrap();
        let (status, _, body) = request(&app, "/collections/pand/items?limit=2").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["numberReturned"], 2);
        let got: Vec<_> = body["features"].as_array().unwrap().to_vec();
        assert_eq!(got, d.features[..2]);
        assert_eq!(
            request(&app, "/collections/absent").await.0,
            StatusCode::NOT_FOUND
        );
    }
}

#[tokio::test]
async fn startup_and_readiness() {
    let d = Dataset::new();
    let server = serve(std::fs::read(d.file.path()).unwrap()).await;
    let app = create_app_with_config(d.config(server.url.clone()))
        .await
        .unwrap();
    assert_eq!(request(&app, "/readyz").await.0, StatusCode::OK);
    server.unavailable.store(true, Ordering::SeqCst);
    assert_eq!(
        request(&app, "/readyz").await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert!(create_app_with_config(d.config(server.url.clone()))
        .await
        .is_err());
    server.unavailable.store(false, Ordering::SeqCst);
    assert_eq!(request(&app, "/readyz").await.0, StatusCode::OK);
    assert!(create_app_with_config(d.config("/no/such/dataset".into()))
        .await
        .is_err());
    let app = create_app_with_config(d.config(d.file.path().to_str().unwrap().into()))
        .await
        .unwrap();
    std::fs::write(d.file.path(), b"broken").unwrap();
    assert_eq!(
        request(&app, "/readyz").await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
async fn query_validation_and_empty_pages() {
    let d = Dataset::new();
    let server = serve(std::fs::read(d.file.path()).unwrap()).await;
    for url in [d.file.path().to_str().unwrap().into(), server.url.clone()] {
        let app = create_app_with_config(d.config(url)).await.unwrap();
        for query in [
            "offset=-1",
            "limit=0",
            "limit=-1",
            "offset=18446744073709551616",
            "limit=x",
            "bbox=0,0,1,1&filter=height%3E1",
        ] {
            assert_eq!(
                request(&app, &format!("/collections/pand/items?{query}"))
                    .await
                    .0,
                StatusCode::BAD_REQUEST,
                "{query}"
            );
        }
        let (status, _, body) = request(&app, "/collections/pand/items?offset=99").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["numberReturned"], 0);
    }
}

#[tokio::test]
async fn missing_ids_and_storage_errors() {
    let d = Dataset::new();
    let server = serve(std::fs::read(d.file.path()).unwrap()).await;
    for url in [d.file.path().to_str().unwrap().into(), server.url.clone()] {
        let app = create_app_with_config(d.config(url.clone())).await.unwrap();
        let uri = format!(
            "/collections/pand/items/{}",
            d.features[0]["id"].as_str().unwrap()
        );
        assert_eq!(request(&app, &uri).await.0, StatusCode::OK);
        let (status, _, body) = request(&app, "/collections/pand/items/absent").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["code"], "404");
        if url.starts_with("http") {
            server.unavailable.store(true, Ordering::SeqCst);
        } else {
            std::fs::rename(d.file.path(), d.file.path().with_extension("hidden")).unwrap();
        }
        for path in [&*uri, "/collections/pand/items"] {
            assert_eq!(request(&app, path).await.0, StatusCode::SERVICE_UNAVAILABLE);
        }
        if url.starts_with("http") {
            server.unavailable.store(false, Ordering::SeqCst);
        } else {
            std::fs::rename(d.file.path().with_extension("hidden"), d.file.path()).unwrap();
        }
    }
}

#[tokio::test]
async fn flask_metadata_and_provenance() {
    let d = Dataset::new();
    let server = serve(std::fs::read(d.file.path()).unwrap()).await;
    for url in [d.file.path().to_str().unwrap().into(), server.url.clone()] {
        let mut config = d.config(url);
        config.collection_version = Some("release-test".into());
        let app = create_app_with_config(config).await.unwrap();
        let (_, _, body) = request(&app, "/collections/pand/items").await;
        assert_eq!(body["metadata"], d.metadata);
        assert_eq!(body["features"], json!(d.features));
        let (_, _, empty) = request(&app, "/collections/pand/items?offset=999").await;
        assert_eq!(empty["metadata"], d.metadata);
        let (_, _, item) = request(
            &app,
            &format!(
                "/collections/pand/items/{}",
                d.features[0]["id"].as_str().unwrap()
            ),
        )
        .await;
        assert_eq!(item["metadata"], d.metadata);
        assert_eq!(item["feature"], d.features[0]);
        assert!(item["links"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["rel"] == "parent"));
        let (_, _, collection) = request(&app, "/collections/pand").await;
        assert_eq!(collection["cityjson"]["transform"], d.metadata["transform"]);
        assert_eq!(collection["version"]["collection"], "release-test");
        assert_eq!(
            collection["extent"]["spatial"]["bbox"],
            json!([[85000.25, 446000.5, 85000.45, 446000.9]])
        );
        assert_eq!(
            request(&app, "/collections").await.2["collections"][0],
            collection
        );
    }
}

#[tokio::test]
async fn crs_validation_headers_and_axis_order() {
    let d = Dataset::new();
    let server = serve(std::fs::read(d.file.path()).unwrap()).await;
    for url in [d.file.path().to_str().unwrap().into(), server.url.clone()] {
        let app = create_app_with_config(d.config(url)).await.unwrap();
        let item = format!(
            "/collections/pand/items/{}",
            d.features[0]["id"].as_str().unwrap()
        );
        for path in ["/collections/pand/items", item.as_str()] {
            for alias in [
                "EPSG:7415",
                "http://www.opengis.net/def/crs/EPSG/0/7415",
                "https://www.opengis.net/def/crs/EPSG/0/7415",
            ] {
                let (status, headers, _) =
                    request(&app, &format!("{path}?crs={}", urlencoding::encode(alias))).await;
                assert_eq!(status, StatusCode::OK);
                assert_eq!(
                    headers["content-crs"],
                    "<http://www.opengis.net/def/crs/EPSG/0/7415>"
                );
            }
            for crs in ["EPSG:4326", "EPSG:28992", "bogus"] {
                assert_eq!(
                    request(&app, &format!("{path}?crs={crs}")).await.0,
                    StatusCode::BAD_REQUEST
                );
            }
        }
        for bbox in ["NaN,0,1,1", "0,0,inf,1", "1,0,0,1", "0,0,1", "0,0,5,1,1,4"] {
            assert_eq!(
                request(&app, &format!("/collections/pand/items?bbox={bbox}"))
                    .await
                    .0,
                StatusCode::BAD_REQUEST
            );
        }
        let a=request(&app,"/collections/pand/items?bbox=4.2,51.9,4.5,52.1&bbox-crs=http%3A%2F%2Fwww.opengis.net%2Fdef%2Fcrs%2FOGC%2F1.3%2FCRS84").await;
        let b=request(&app,"/collections/pand/items?bbox=51.9,4.2,52.1,4.5&bbox-crs=https%3A%2F%2Fwww.opengis.net%2Fdef%2Fcrs%2FEPSG%2F0%2F4326").await;
        assert_eq!(a.0, StatusCode::OK);
        assert_eq!(b.0, StatusCode::OK);
        assert_eq!(a.2["features"], json!(d.features));
        assert_eq!(a.2["features"], b.2["features"]);
        for f in ["json", "cityjson", "cjseq", "obj"] {
            let r = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(format!("/collections/pand/items?f={f}"))
                        .header("origin", "http://viewer.test")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(r.status(), StatusCode::OK);
            assert!(r.headers().contains_key("content-crs"));
            assert!(r.headers()["access-control-expose-headers"]
                .to_str()
                .unwrap()
                .contains("content-crs"));
            let bytes = axum::body::to_bytes(r.into_body(), usize::MAX)
                .await
                .unwrap();
            if f == "cjseq" {
                let text = std::str::from_utf8(&bytes).unwrap();
                let mut lines = text.lines();
                assert_eq!(
                    serde_json::from_str::<Value>(lines.next().unwrap()).unwrap(),
                    d.metadata
                );
                assert_eq!(
                    lines
                        .map(|l| serde_json::from_str::<Value>(l).unwrap())
                        .collect::<Vec<_>>(),
                    d.features
                );
            }
        }
    }
}

#[tokio::test]
async fn pagination_preserves_features_and_filter() {
    let d = Dataset::new();
    let server = serve(std::fs::read(d.file.path()).unwrap()).await;
    for url in [d.file.path().to_str().unwrap().into(), server.url.clone()] {
        let app = create_app_with_config(d.config(url)).await.unwrap();
        for selection in ["", "&bbox=84000,445000,86000,447000", "&filter=height%3E0"] {
            let mut all = vec![];
            for offset in 0..3 {
                let (status, _, body) = request(
                    &app,
                    &format!("/collections/pand/items?limit=1&offset={offset}{selection}"),
                )
                .await;
                assert_eq!(status, StatusCode::OK);
                assert_eq!(body["numberMatched"], 3);
                all.extend(body["features"].as_array().unwrap().iter().cloned());
            }
            assert_eq!(all, d.features);
        }
        let (_, _, body) = request(&app, "/collections/pand/items?filter=height%3E0&limit=1").await;
        for link in body["links"].as_array().unwrap() {
            let url = reqwest::Url::parse(link["href"].as_str().unwrap()).unwrap();
            assert_eq!(
                url.query_pairs().find(|(k, _)| k == "filter").unwrap().1,
                "height>0"
            );
        }
    }
}

#[tokio::test]
async fn startup_configuration_and_remote_probe_timeout() {
    let d = Dataset::new();
    for (cap, timeout) in [(0, 10), (100, 0)] {
        let mut config = d.config(d.file.path().to_str().unwrap().into());
        config.max_return_features = cap;
        config.probe_timeout_seconds = timeout;
        assert!(create_app_with_config(config).await.is_err());
    }
    let empty = Dataset::with_count(0);
    let app = create_app_with_config(empty.config(empty.file.path().to_str().unwrap().into()))
        .await
        .unwrap();
    let (status, _, body) = request(&app, "/collections/pand/items").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["features"], json!([]));
    assert!(request(&app, "/collections/pand").await.2["version"]
        .get("collection")
        .is_none());
    let server = serve(std::fs::read(d.file.path()).unwrap()).await;
    let mut config = d.config(server.url.clone());
    config.probe_timeout_seconds = 1;
    let app = create_app_with_config(config.clone()).await.unwrap();
    server.unavailable.store(true, Ordering::SeqCst);
    for status in [404, 500] {
        server.failure_status.store(status, Ordering::SeqCst);
        assert_eq!(
            request(&app, "/collections/pand/items").await.0,
            StatusCode::SERVICE_UNAVAILABLE
        );
    }
    server.unavailable.store(false, Ordering::SeqCst);
    server.delay.store(true, Ordering::SeqCst);
    assert!(create_app_with_config(config).await.is_err());
    assert_eq!(
        request(&app, "/readyz").await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
async fn missing_id_index_is_storage_failure_and_invalid_crs_fails_startup() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../conformance");
    let d = Dataset::new();
    let config = d.config(root.join("noise_extension.fcb").to_str().unwrap().into());
    assert!(create_app_with_config(config).await.is_err());
    let empty = Dataset::with_count(0);
    let app = create_app_with_config(empty.config(empty.file.path().to_str().unwrap().into()))
        .await
        .unwrap();
    assert_eq!(
        request(&app, "/collections/pand/items/nonexistent").await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
}
