//! Coordinate Reference System (CRS) transformation utilities
//!
//! This module provides functionality to transform bounding boxes between different
//! coordinate reference systems. The primary use case is transforming input bounding
//! boxes from various CRS (like WGS84) to the Dutch RD New coordinate system (EPSG:28992).

use proj::Proj;
use thiserror::Error;

/// The Dutch RD New coordinate system (Rijksdriehoekscoördinaten)
pub const DUTCH_CRS: &str = "EPSG:28992";

/// WGS84 coordinate system (commonly used in GPS and web mapping)
pub const WGS84_CRS: &str = "EPSG:4326";

#[derive(Debug, Error)]
pub enum CrsError {
    #[error("Failed to create projection from {from} to {to}: {source}")]
    ProjectionCreationFailed {
        from: String,
        to: String,
        #[source]
        source: proj::ProjCreateError,
    },

    #[error("Failed to transform coordinates from {from} to {to}: {source}")]
    TransformationFailed {
        from: String,
        to: String,
        #[source]
        source: proj::ProjError,
    },

    #[error("Invalid bounding box: expected 4 coordinates [minx, miny, maxx, maxy], got {0}")]
    InvalidBboxLength(usize),
}

/// Transform a bounding box from one CRS to another.
///
/// # Arguments
/// * `bbox` - A bounding box as [minx, miny, maxx, maxy]
/// * `from_crs` - Source coordinate reference system (e.g., "EPSG:4326")
/// * `to_crs` - Target coordinate reference system (e.g., "EPSG:28992")
///
/// # Returns
/// * `Ok(Vec<f64>)` - Transformed bounding box as [minx, miny, maxx, maxy]
/// * `Err(CrsError)` - If transformation fails
///
/// # Example
/// ```ignore
/// let wgs84_bbox = vec![4.8, 52.3, 4.9, 52.4]; // Amsterdam area in WGS84
/// let rd_bbox = transform_bbox(&wgs84_bbox, WGS84_CRS, DUTCH_CRS)?;
/// ```
pub fn transform_bbox(bbox: &[f64], from_crs: &str, to_crs: &str) -> Result<Vec<f64>, CrsError> {
    if bbox.len() != 4 {
        return Err(CrsError::InvalidBboxLength(bbox.len()));
    }

    // If source and target CRS are the same, return bbox as-is
    if from_crs == to_crs {
        return Ok(bbox.to_vec());
    }

    // Create projection
    let proj = Proj::new_known_crs(from_crs, to_crs, None).map_err(|e| {
        CrsError::ProjectionCreationFailed {
            from: from_crs.to_string(),
            to: to_crs.to_string(),
            source: e,
        }
    })?;

    let (minx, miny, maxx, maxy) = (bbox[0], bbox[1], bbox[2], bbox[3]);

    // Transform the lower-left corner (minx, miny)
    let (transformed_minx, transformed_miny) =
        proj.convert((minx, miny))
            .map_err(|e| CrsError::TransformationFailed {
                from: from_crs.to_string(),
                to: to_crs.to_string(),
                source: e,
            })?;

    // Transform the upper-right corner (maxx, maxy)
    let (transformed_maxx, transformed_maxy) =
        proj.convert((maxx, maxy))
            .map_err(|e| CrsError::TransformationFailed {
                from: from_crs.to_string(),
                to: to_crs.to_string(),
                source: e,
            })?;

    // Ensure min/max are in correct order after transformation
    let final_minx = transformed_minx.min(transformed_maxx);
    let final_maxx = transformed_minx.max(transformed_maxx);
    let final_miny = transformed_miny.min(transformed_maxy);
    let final_maxy = transformed_miny.max(transformed_maxy);

    Ok(vec![final_minx, final_miny, final_maxx, final_maxy])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_wgs84_to_dutch_rd() {
        // Amsterdam Central Station area in WGS84
        let wgs84_bbox = vec![4.895, 52.375, 4.905, 52.385];

        let result = transform_bbox(&wgs84_bbox, WGS84_CRS, DUTCH_CRS);
        assert!(result.is_ok());

        let rd_bbox = result.unwrap();
        println!("Transformed bbox: {rd_bbox:?}");

        // RD coordinates for Amsterdam should be roughly in the range of 120000-122000, 487000-489000
        assert!(
            rd_bbox[0] > 119000.0 && rd_bbox[0] < 123000.0,
            "minx out of range: {}",
            rd_bbox[0]
        );
        assert!(
            rd_bbox[1] > 486000.0 && rd_bbox[1] < 490000.0,
            "miny out of range: {}",
            rd_bbox[1]
        );
        assert!(
            rd_bbox[2] > 119000.0 && rd_bbox[2] < 123000.0,
            "maxx out of range: {}",
            rd_bbox[2]
        );
        assert!(
            rd_bbox[3] > 486000.0 && rd_bbox[3] < 490000.0,
            "maxy out of range: {}",
            rd_bbox[3]
        );
    }

    #[test]
    fn test_transform_same_crs() {
        let bbox = vec![120000.0, 487000.0, 121000.0, 488000.0];
        let result = transform_bbox(&bbox, DUTCH_CRS, DUTCH_CRS);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), bbox);
    }

    #[test]
    fn test_invalid_bbox_length() {
        let bbox = vec![4.895, 52.375]; // Only 2 coordinates
        let result = transform_bbox(&bbox, WGS84_CRS, DUTCH_CRS);
        assert!(matches!(result, Err(CrsError::InvalidBboxLength(2))));
    }

    #[test]
    fn test_min_max_order_preserved() {
        // Test with coordinates that might flip during transformation
        let bbox = vec![4.895, 52.375, 4.905, 52.385];
        let result = transform_bbox(&bbox, WGS84_CRS, DUTCH_CRS).unwrap();

        // Ensure min < max after transformation
        assert!(result[0] < result[2]); // minx < maxx
        assert!(result[1] < result[3]); // miny < maxy
    }
}
