//! C FFI layer for FlatCityBuf Go bindings.
//!
//! Provides C-compatible functions for reading FCB files from Go via CGO.
//! All functions follow the pattern of returning a status code (0 = success)
//! with output via pointer parameters, or returning opaque pointer types.

use fcb_core::{FcbReader, SpatialQuery};
use std::ffi::{c_char, CStr, CString};
use std::fs::File;
use std::io::BufReader;
use std::ptr;

/// Opaque reader type exposed to C/Go
pub struct FcbFileReader {
    inner: FcbReader<BufReader<File>>,
}

/// Opaque iterator type exposed to C/Go
pub struct FcbFileIterator {
    inner: Box<dyn IteratorHelper + Send>,
    features_count: u64,
}

/// Trait for type-erased iterator operations (same pattern as C++ bindings)
trait IteratorHelper {
    fn advance(&mut self) -> Result<bool, String>;
    fn current_json(&self) -> Result<*mut c_char, String>;
    fn current_id(&self) -> Result<*mut c_char, String>;
}

/// Concrete implementation wrapping the seekable FeatureIter
struct SeekableIterHelper {
    iter: fcb_core::FeatureIter<BufReader<File>, fcb_core::reader_trait::Seekable>,
    cur_id: Option<String>,
    cur_json: Option<String>,
    finished: bool,
}

// Single-threaded access from Go; safe for our use case.
unsafe impl Send for SeekableIterHelper {}

impl IteratorHelper for SeekableIterHelper {
    fn advance(&mut self) -> Result<bool, String> {
        if self.finished {
            return Ok(false);
        }

        match self.iter.next() {
            Ok(Some(_)) => {
                let cj_feature = self
                    .iter
                    .cur_cj_feature()
                    .map_err(|e| format!("Failed to get feature: {e}"))?;
                self.cur_id = Some(cj_feature.id.clone());
                self.cur_json = Some(
                    serde_json::to_string(&cj_feature)
                        .map_err(|e| format!("Failed to serialize: {e}"))?,
                );
                Ok(true)
            }
            Ok(None) => {
                self.finished = true;
                self.cur_id = None;
                self.cur_json = None;
                Ok(false)
            }
            Err(e) => Err(format!("Failed to advance: {e}")),
        }
    }

    fn current_json(&self) -> Result<*mut c_char, String> {
        match &self.cur_json {
            Some(json) => CString::new(json.as_str())
                .map(|cs| cs.into_raw())
                .map_err(|e| format!("Invalid JSON string: {e}")),
            None => Err("No current feature".to_string()),
        }
    }

    fn current_id(&self) -> Result<*mut c_char, String> {
        match &self.cur_id {
            Some(id) => CString::new(id.as_str())
                .map(|cs| cs.into_raw())
                .map_err(|e| format!("Invalid ID string: {e}")),
            None => Err("No current feature".to_string()),
        }
    }
}

// ============ Reader API ============

/// Open an FCB file for reading. Returns null on error.
/// On error, `error_out` is set to an error message (caller must free with `fcb_free_string`).
#[no_mangle]
pub unsafe extern "C" fn fcb_reader_open(
    path: *const c_char,
    error_out: *mut *mut c_char,
) -> *mut FcbFileReader {
    let path_str = match CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(e) => {
            set_error(error_out, &format!("Invalid UTF-8 path: {e}"));
            return ptr::null_mut();
        }
    };

    let file = match File::open(path_str) {
        Ok(f) => f,
        Err(e) => {
            set_error(error_out, &format!("Failed to open file: {e}"));
            return ptr::null_mut();
        }
    };

    let buf_reader = BufReader::new(file);
    match FcbReader::open(buf_reader) {
        Ok(reader) => Box::into_raw(Box::new(FcbFileReader { inner: reader })),
        Err(e) => {
            set_error(error_out, &format!("Failed to parse FCB header: {e}"));
            ptr::null_mut()
        }
    }
}

/// Get the feature count from an open reader.
#[no_mangle]
pub unsafe extern "C" fn fcb_reader_features_count(reader: *const FcbFileReader) -> u64 {
    if reader.is_null() {
        return 0;
    }
    (*reader).inner.header().features_count()
}

/// Check if the reader has a spatial index.
#[no_mangle]
pub unsafe extern "C" fn fcb_reader_has_spatial_index(reader: *const FcbFileReader) -> bool {
    if reader.is_null() {
        return false;
    }
    (*reader).inner.header().index_node_size() > 0
}

/// Get CityJSON metadata as a JSON string.
/// Caller must free the returned string with `fcb_free_string`.
#[no_mangle]
pub unsafe extern "C" fn fcb_reader_cityjson_metadata(
    reader: *const FcbFileReader,
    error_out: *mut *mut c_char,
) -> *mut c_char {
    if reader.is_null() {
        set_error(error_out, "Null reader pointer");
        return ptr::null_mut();
    }
    let header = (*reader).inner.header();
    match fcb_core::deserializer::to_cj_metadata(&header) {
        Ok(cj) => match serde_json::to_string(&cj) {
            Ok(json) => CString::new(json).unwrap_or_default().into_raw(),
            Err(e) => {
                set_error(error_out, &format!("Serialization error: {e}"));
                ptr::null_mut()
            }
        },
        Err(e) => {
            set_error(error_out, &format!("Metadata error: {e}"));
            ptr::null_mut()
        }
    }
}

// ============ Selection API ============

/// Select all features. Consumes the reader.
/// Returns null on error.
#[no_mangle]
pub unsafe extern "C" fn fcb_reader_select_all(
    reader: *mut FcbFileReader,
    error_out: *mut *mut c_char,
) -> *mut FcbFileIterator {
    if reader.is_null() {
        set_error(error_out, "Null reader pointer");
        return ptr::null_mut();
    }
    let reader = Box::from_raw(reader);
    match reader.inner.select_all() {
        Ok(iter) => {
            let features_count = iter.features_count().unwrap_or(0) as u64;
            let helper = SeekableIterHelper {
                iter,
                cur_id: None,
                cur_json: None,
                finished: false,
            };
            Box::into_raw(Box::new(FcbFileIterator {
                inner: Box::new(helper),
                features_count,
            }))
        }
        Err(e) => {
            set_error(error_out, &format!("Failed to select all: {e}"));
            ptr::null_mut()
        }
    }
}

/// Select features within a bounding box. Consumes the reader.
/// Returns null on error.
#[no_mangle]
pub unsafe extern "C" fn fcb_reader_select_bbox(
    reader: *mut FcbFileReader,
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
    error_out: *mut *mut c_char,
) -> *mut FcbFileIterator {
    if reader.is_null() {
        set_error(error_out, "Null reader pointer");
        return ptr::null_mut();
    }
    let reader = Box::from_raw(reader);
    let query = SpatialQuery::BBox(min_x, min_y, max_x, max_y);
    match reader.inner.select_query(query, None, None) {
        Ok(iter) => {
            let features_count = iter.features_count().unwrap_or(0) as u64;
            let helper = SeekableIterHelper {
                iter,
                cur_id: None,
                cur_json: None,
                finished: false,
            };
            Box::into_raw(Box::new(FcbFileIterator {
                inner: Box::new(helper),
                features_count,
            }))
        }
        Err(e) => {
            set_error(error_out, &format!("Failed to select bbox: {e}"));
            ptr::null_mut()
        }
    }
}

// ============ Iterator API ============

/// Advance to the next feature. Returns 1 if a feature is available, 0 if done, -1 on error.
#[no_mangle]
pub unsafe extern "C" fn fcb_iterator_next(
    iter: *mut FcbFileIterator,
    error_out: *mut *mut c_char,
) -> i32 {
    if iter.is_null() {
        set_error(error_out, "Null iterator pointer");
        return -1;
    }
    match (*iter).inner.advance() {
        Ok(true) => 1,
        Ok(false) => 0,
        Err(e) => {
            set_error(error_out, &e);
            -1
        }
    }
}

/// Get the current feature as a JSON string.
/// Caller must free the returned string with `fcb_free_string`.
#[no_mangle]
pub unsafe extern "C" fn fcb_iterator_current_json(
    iter: *const FcbFileIterator,
    error_out: *mut *mut c_char,
) -> *mut c_char {
    if iter.is_null() {
        set_error(error_out, "Null iterator pointer");
        return ptr::null_mut();
    }
    match (*iter).inner.current_json() {
        Ok(ptr) => ptr,
        Err(e) => {
            set_error(error_out, &e);
            ptr::null_mut()
        }
    }
}

/// Get the current feature ID.
/// Caller must free the returned string with `fcb_free_string`.
#[no_mangle]
pub unsafe extern "C" fn fcb_iterator_current_id(
    iter: *const FcbFileIterator,
    error_out: *mut *mut c_char,
) -> *mut c_char {
    if iter.is_null() {
        set_error(error_out, "Null iterator pointer");
        return ptr::null_mut();
    }
    match (*iter).inner.current_id() {
        Ok(ptr) => ptr,
        Err(e) => {
            set_error(error_out, &e);
            ptr::null_mut()
        }
    }
}

/// Get the total features count from the iterator.
#[no_mangle]
pub unsafe extern "C" fn fcb_iterator_features_count(iter: *const FcbFileIterator) -> u64 {
    if iter.is_null() {
        return 0;
    }
    (*iter).features_count
}

// ============ Memory Management ============

/// Free a reader. Must be called when done with the reader.
#[no_mangle]
pub unsafe extern "C" fn fcb_reader_free(reader: *mut FcbFileReader) {
    if !reader.is_null() {
        drop(Box::from_raw(reader));
    }
}

/// Free an iterator. Must be called when done with the iterator.
#[no_mangle]
pub unsafe extern "C" fn fcb_iterator_free(iter: *mut FcbFileIterator) {
    if !iter.is_null() {
        drop(Box::from_raw(iter));
    }
}

/// Free a C string returned by any fcb_ function.
#[no_mangle]
pub unsafe extern "C" fn fcb_free_string(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

// ============ Helper ============

unsafe fn set_error(error_out: *mut *mut c_char, msg: &str) {
    if !error_out.is_null() {
        if let Ok(cs) = CString::new(msg) {
            *error_out = cs.into_raw();
        }
    }
}
