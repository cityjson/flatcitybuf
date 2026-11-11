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
        atomic::{AtomicBool, Ordering},
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
        let header = json!({"type":"CityJSON","version":"2.0","transform":{"scale":[0.01,0.02,0.03],"translate":[85000.25,446000.5,3.0]},"CityObjects":{},"vertices":[],"metadata":{"referenceSystem":"https://www.opengis.net/def/crs/EPSG/0/7415","title":"API fixture"}});
        let mut input = format!("{header}\n");
        for i in 0..3 {
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
                feature_count: 3,
                index_node_size: 16,
                attribute_indices: Some(vec![
                    ("identificatie".into(), None),
                    ("height".into(), None),
                ]),
                geographical_extent: None,
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
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn serve(data: Vec<u8>) -> Server {
    use axum::{
        extract::State,
        http::{header, HeaderMap},
        response::IntoResponse,
        routing::get,
    };
    async fn range(
        State((data, down)): State<(Arc<Vec<u8>>, Arc<AtomicBool>)>,
        headers: HeaderMap,
    ) -> axum::response::Response {
        if down.load(Ordering::SeqCst) {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
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
    let app = Router::new()
        .route("/data", get(range))
        .with_state((Arc::new(data), unavailable.clone()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/data", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    Server {
        url,
        unavailable,
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
        let got: Vec<_> = body["features"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["feature"].clone())
            .collect();
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
