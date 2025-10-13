// Constants for the 3DBAG API
// These constants match the values from the original 3DBAG API parameters.py

// CRS (Coordinate Reference System)
pub const STORAGE_CRS: &str = "http://www.opengis.net/def/crs/EPSG/0/7415";
pub const CRS84: &str = "http://www.opengis.net/def/crs/OGC/1.3/CRS84";
pub const EPSG_28992: &str = "http://www.opengis.net/def/crs/EPSG/0/28992";

// Default bounding box for the Netherlands (in RD coordinates)
pub const DEFAULT_BBOX: [f64; 4] = [10000.0, 306250.0, 287760.0, 623690.0];

// Pagination defaults
pub const DEFAULT_OFFSET: i32 = 1;
pub const DEFAULT_LIMIT: i32 = 10;
pub const DEFAULT_MAX_LIMIT: i32 = 10000;

// API metadata
pub const API_TITLE: &str = "3DBAG API";
pub const API_DESCRIPTION: &str = "3DBAG is an extended version of the 3DBAG data set. It contains additional information that is either derived from the 3DBAG, or integrated from other data sources.";

// Collection metadata
pub const PAND_COLLECTION_ID: &str = "pand";
pub const PAND_COLLECTION_TITLE: &str = "Pand";
pub const PAND_COLLECTION_DESCRIPTION: &str =
    "3D building models based on the 'pand' layer of the BAG dataset.";

// Version information
pub const API_VERSION: &str = "0.1";
pub const COLLECTION_VERSION: &str = "v2023.10.08";

// Content types
pub const CONTENT_TYPE_JSON: &str = "application/json";
pub const CONTENT_TYPE_GEOJSON: &str = "application/geo+json";
pub const CONTENT_TYPE_CITYJSON: &str = "application/city+json";
pub const CONTENT_TYPE_HTML: &str = "text/html";
pub const CONTENT_TYPE_RDF_XML: &str = "application/rdf+xml";
pub const CONTENT_TYPE_OPENAPI: &str = "application/vnd.oai.openapi+json;version=3.0";

// License
pub const LICENSE_URL: &str = "https://creativecommons.org/licenses/by/4.0/";
pub const LICENSE_RDF_URL: &str = "https://creativecommons.org/licenses/by/4.0/rdf";
pub const LICENSE_TITLE: &str = "CC BY 4.0";

// Conformance
pub const CITYJSON_SPEC: &str = "https://cityjson.org/specs/1.1.1/";

// Link relations
pub const REL_SELF: &str = "self";
pub const REL_SERVICE_DESC: &str = "service-desc";
pub const REL_SERVICE_DOC: &str = "service-doc";
pub const REL_CONFORMANCE: &str = "conformance";
pub const REL_DATA: &str = "data";
pub const REL_ITEMS: &str = "items";
pub const REL_LICENSE: &str = "license";
pub const REL_COLLECTION: &str = "collection";
pub const REL_PARENT: &str = "parent";
pub const REL_CHILD: &str = "child";

// Link titles
pub const TITLE_THIS_DOCUMENT: &str = "this document";
pub const TITLE_API_DEFINITION: &str = "the API definition";
pub const TITLE_API_DOCUMENTATION: &str = "the API documentation";
pub const TITLE_CONFORMANCE: &str = "Conformance classes implemented by this server";
pub const TITLE_COLLECTIONS: &str = "Information about the feature collections";
pub const TITLE_PAND_ITEMS: &str = "Pand items";

// Item type
pub const ITEM_TYPE_FEATURE: &str = "feature";

// Feature collection type
pub const FEATURE_COLLECTION_TYPE: &str = "FeatureCollection";

// CityJSON Metadata
pub const CITYJSON_VERSION: &str = "2.0";

pub const CITYJSON_SCALE: [f64; 3] = [0.001, 0.001, 0.001];
pub const CITYJSON_TRANSLATE: [f64; 3] = [171800.0, 472700.0, 0.0];

pub const CITYJSON_EXTENSIONS: Vec<String> = vec![];

// CRS, these are not used as part of API responses, but are used in the code.

/// Dutch RD New coordinate system (Rijksdriehoekscoördinaten)
pub const DUTCH_CRS: &str = "EPSG:28992";
/// WGS84 coordinate system (commonly used in GPS and web mapping)
pub const WGS84_CRS: &str = "EPSG:4326";
