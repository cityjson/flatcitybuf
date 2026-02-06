//! Reader bindings for C++

use crate::ffi::{BoundingBox, CityFeatureData, FcbMetadata};
use fcb_core::{FcbReader, SpatialQuery};
use std::fs::File;
use std::io::BufReader;

/// Wrapper around FcbReader for C++ interop
pub struct FcbFileReader {
    inner: FcbReader<BufReader<File>>,
}

/// Wrapper that holds the iterator state
/// Uses type erasure to hide the internal Seekable type parameter
pub struct FcbFileReaderIterator {
    inner: Box<dyn IteratorHelper + Send>,
    has_current: bool,
    features_count: Option<usize>,
}

/// Trait for type-erased iterator operations
trait IteratorHelper {
    fn next_feature(&mut self) -> Result<bool, String>;
    fn current_feature_json(&self) -> Result<CityFeatureData, String>;
    fn count(&self) -> Option<usize>;
}

/// Wrapper to help with type erasure
struct IteratorWrapper {
    iter: fcb_core::FeatureIter<BufReader<File>, fcb_core::reader_trait::Seekable>,
}

// We're using single-threaded access, so this is safe for our use case.
// C++ bindings will be called from a single thread.
unsafe impl Send for IteratorWrapper {}

struct DirectIteratorHelper {
    iter_inner: Option<IteratorWrapper>,
    cur_feature: Option<(String, String)>,
    features_count: Option<usize>,
    finished: bool,
}

unsafe impl Send for DirectIteratorHelper {}

impl IteratorHelper for DirectIteratorHelper {
    fn next_feature(&mut self) -> Result<bool, String> {
        if self.finished {
            return Ok(false);
        }

        let wrapper = self.iter_inner.as_mut().ok_or("Iterator consumed")?;

        match wrapper.iter.next() {
            Ok(Some(_)) => {
                // Get CityJSON feature
                let cj_feature = wrapper
                    .iter
                    .cur_cj_feature()
                    .map_err(|e| format!("Failed to get feature: {}", e))?;

                // cj_feature.id is String (the feature ID)
                let id = cj_feature.id.clone();
                let json = serde_json::to_string(&cj_feature)
                    .map_err(|e| format!("Failed to serialize: {}", e))?;

                self.cur_feature = Some((id, json));
                Ok(true)
            }
            Ok(None) => {
                self.finished = true;
                self.cur_feature = None;
                Ok(false)
            }
            Err(e) => Err(format!("Failed to advance: {}", e)),
        }
    }

    fn current_feature_json(&self) -> Result<CityFeatureData, String> {
        match &self.cur_feature {
            Some((id, json)) => Ok(CityFeatureData {
                id: id.clone(),
                json: json.clone(),
            }),
            None => Err("No current feature".to_string()),
        }
    }

    fn count(&self) -> Option<usize> {
        self.features_count
    }
}

/// Open an FCB file for reading
pub fn fcb_reader_open(path: &str) -> Result<Box<FcbFileReader>, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;
    let buf_reader = BufReader::new(file);
    let fcb_reader =
        FcbReader::open(buf_reader).map_err(|e| format!("Failed to parse FCB header: {}", e))?;

    Ok(Box::new(FcbFileReader { inner: fcb_reader }))
}

/// Get metadata from a reader
pub fn fcb_reader_metadata(reader: &FcbFileReader) -> FcbMetadata {
    let header = reader.inner.header();

    FcbMetadata {
        version: 1, // TODO: Extract from header when available
        features_count: header.features_count(),
        has_spatial_index: header.index_node_size() > 0,
        has_attribute_index: header
            .attribute_index()
            .map(|a| !a.is_empty())
            .unwrap_or(false),
    }
}

/// Select all features for iteration
pub fn fcb_reader_select_all(
    reader: Box<FcbFileReader>,
) -> Result<Box<FcbFileReaderIterator>, String> {
    let iter = reader
        .inner
        .select_all()
        .map_err(|e| format!("Failed to select all features: {}", e))?;

    let features_count = iter.features_count();

    let helper: Box<dyn IteratorHelper + Send> = Box::new(DirectIteratorHelper {
        iter_inner: Some(IteratorWrapper { iter }),
        cur_feature: None,
        features_count,
        finished: false,
    });

    Ok(Box::new(FcbFileReaderIterator {
        inner: helper,
        has_current: false,
        features_count,
    }))
}

/// Select features within a bounding box
pub fn fcb_reader_select_bbox(
    reader: Box<FcbFileReader>,
    bbox: BoundingBox,
) -> Result<Box<FcbFileReaderIterator>, String> {
    let query = SpatialQuery::BBox(bbox.min_x, bbox.min_y, bbox.max_x, bbox.max_y);

    let iter = reader
        .inner
        .select_query(query, None, None)
        .map_err(|e| format!("Failed to select features by bbox: {}", e))?;

    let features_count = iter.features_count();

    let helper: Box<dyn IteratorHelper + Send> = Box::new(DirectIteratorHelper {
        iter_inner: Some(IteratorWrapper { iter }),
        cur_feature: None,
        features_count,
        finished: false,
    });

    Ok(Box::new(FcbFileReaderIterator {
        inner: helper,
        has_current: false,
        features_count,
    }))
}

/// Advance to the next feature
pub fn fcb_iterator_next(iter: &mut FcbFileReaderIterator) -> Result<bool, String> {
    let result = iter.inner.next_feature()?;
    iter.has_current = result;
    Ok(result)
}

/// Get the current feature data
pub fn fcb_iterator_current(iter: &FcbFileReaderIterator) -> Result<CityFeatureData, String> {
    if !iter.has_current {
        return Err("No current feature - call next() first".to_string());
    }
    iter.inner.current_feature_json()
}

/// Get the total features count
pub fn fcb_iterator_features_count(iter: &FcbFileReaderIterator) -> u64 {
    iter.features_count.unwrap_or(0) as u64
}
