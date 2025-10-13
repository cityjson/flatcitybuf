//! Coordinate Reference System (CRS) transformation utilities
//!
//! This module provides functionality to transform bounding boxes between different
//! coordinate reference systems. The primary use case is transforming input bounding
//! boxes from various CRS (like WGS84) to the Dutch RD New coordinate system (EPSG:28992).

use proj4rs::proj::Proj;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CrsError {
    #[error("Failed to create projection from {from} to {to}: {source}")]
    ProjectionCreationFailed {
        from: String,
        to: String,
        #[source]
        source: proj4rs::errors::Error,
    },

    #[error("Failed to transform coordinates from {from} to {to}: {source}")]
    TransformationFailed {
        from: String,
        to: String,
        #[source]
        source: proj4rs::errors::Error,
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
pub fn transform_bbox(bbox: &[f64], from_epsg: &str, to_epsg: &str) -> Result<Vec<f64>, CrsError> {
    if bbox.len() != 4 {
        return Err(CrsError::InvalidBboxLength(bbox.len()));
    }

    // If source and target CRS are the same, return bbox as-is
    if from_epsg == to_epsg {
        return Ok(bbox.to_vec());
    }

    // `from_epsg` and `to_epsg` are in the format "EPSG:4326" or "EPSG:28992". This must be converted to proj string.
    let from_code = from_epsg
        .split(':')
        .next_back()
        .ok_or_else(|| CrsError::ProjectionCreationFailed {
            from: from_epsg.to_string(),
            to: to_epsg.to_string(),
            source: proj4rs::errors::Error::InputStringError("Missing ':' in from_epsg"),
        })?
        .parse::<u16>()
        .map_err(|_| CrsError::ProjectionCreationFailed {
            from: from_epsg.to_string(),
            to: to_epsg.to_string(),
            source: proj4rs::errors::Error::InputStringError("from_epsg is not a valid EPSG code"),
        })?;

    let to_code = to_epsg
        .split(':')
        .next_back()
        .ok_or_else(|| CrsError::ProjectionCreationFailed {
            from: from_epsg.to_string(),
            to: to_epsg.to_string(),
            source: proj4rs::errors::Error::InputStringError("Missing ':' in to_epsg"),
        })?
        .parse::<u16>()
        .map_err(|_| CrsError::ProjectionCreationFailed {
            from: from_epsg.to_string(),
            to: to_epsg.to_string(),
            source: proj4rs::errors::Error::InputStringError("to_epsg is not a valid EPSG code"),
        })?;

    let from_proj_string = crs_definitions::from_code(from_code)
        .ok_or_else(|| CrsError::ProjectionCreationFailed {
            from: from_epsg.to_string(),
            to: to_epsg.to_string(),
            source: proj4rs::errors::Error::InputStringError("from_epsg is not a valid EPSG code"),
        })?
        .proj4;
    let to_proj_string = crs_definitions::from_code(to_code)
        .ok_or_else(|| CrsError::ProjectionCreationFailed {
            from: from_epsg.to_string(),
            to: to_epsg.to_string(),
            source: proj4rs::errors::Error::InputStringError("to_epsg is not a valid EPSG code"),
        })?
        .proj4;

    let from_proj = Proj::from_proj_string(from_proj_string).map_err(|e| {
        CrsError::ProjectionCreationFailed {
            from: from_epsg.to_string(),
            to: to_epsg.to_string(),
            source: e,
        }
    })?;
    let to_proj =
        Proj::from_proj_string(to_proj_string).map_err(|e| CrsError::ProjectionCreationFailed {
            from: from_epsg.to_string(),
            to: to_epsg.to_string(),
            source: e,
        })?;

    let (minx, miny, maxx, maxy) = (bbox[0], bbox[1], bbox[2], bbox[3]);

    // Check if source CRS is geographic (has +proj=longlat or +proj=latlong)
    // NOTE: maybe this is not the best way to check if the CRS is geographic. Find a better way to do this. Proj works on radians, so we need to convert to radians if the CRS is geographic.
    let from_is_geographic =
        from_proj_string.contains("+proj=longlat") || from_proj_string.contains("+proj=latlong");

    // Convert to radians if source is geographic (degrees to radians)
    let mut left_lower = if from_is_geographic {
        (minx.to_radians(), miny.to_radians())
    } else {
        (minx, miny)
    };

    let mut right_upper = if from_is_geographic {
        (maxx.to_radians(), maxy.to_radians())
    } else {
        (maxx, maxy)
    };

    // Transform the lower-left corner (minx, miny)
    proj4rs::transform::transform(&from_proj, &to_proj, &mut left_lower).map_err(|e| {
        CrsError::TransformationFailed {
            from: from_epsg.to_string(),
            to: to_epsg.to_string(),
            source: e,
        }
    })?;

    // Transform the upper-right corner (maxx, maxy)
    proj4rs::transform::transform(&from_proj, &to_proj, &mut right_upper).map_err(|e| {
        CrsError::TransformationFailed {
            from: from_epsg.to_string(),
            to: to_epsg.to_string(),
            source: e,
        }
    })?;

    // Check if destination CRS is geographic and convert back to degrees if needed
    // NOTE: maybe this is not the best way to check if the CRS is geographic.
    let to_is_geographic =
        to_proj_string.contains("+proj=longlat") || to_proj_string.contains("+proj=latlong");

    if to_is_geographic {
        left_lower.0 = left_lower.0.to_degrees();
        left_lower.1 = left_lower.1.to_degrees();
        right_upper.0 = right_upper.0.to_degrees();
        right_upper.1 = right_upper.1.to_degrees();
    }

    // Ensure min/max are in correct order after transformation
    let final_minx = left_lower.0.min(right_upper.0);
    let final_maxx = left_lower.0.max(right_upper.0);
    let final_miny = left_lower.1.min(right_upper.1);
    let final_maxy = left_lower.1.max(right_upper.1);

    Ok(vec![final_minx, final_miny, final_maxx, final_maxy])
}

#[cfg(test)]
mod tests {
    use crate::constants::{DUTCH_CRS, WGS84_CRS};

    use super::*;

    #[test]
    fn test_transform_wgs84_to_dutch_rd() {
        // Amsterdam Central Station area in WGS84
        let wgs84_bbox = vec![4.895, 52.375, 4.905, 52.385];

        let result = transform_bbox(&wgs84_bbox, WGS84_CRS, DUTCH_CRS);
        println!("Result: {result:?}");
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

    #[test]
    fn test_round_trip_transformation() {
        // Original WGS84 bbox - Amsterdam Central Station area
        let original_bbox = vec![4.895, 52.375, 4.905, 52.385];

        // Transform to Dutch RD
        let dutch_bbox = transform_bbox(&original_bbox, WGS84_CRS, DUTCH_CRS)
            .expect("Failed to transform WGS84 to Dutch RD");

        // Transform back to WGS84
        let round_trip_bbox = transform_bbox(&dutch_bbox, DUTCH_CRS, WGS84_CRS)
            .expect("Failed to transform Dutch RD back to WGS84");

        // Compare with original - should be very close (within reasonable floating point precision)
        const EPSILON: f64 = 1e-6; // Tolerance for floating point comparison

        assert!(
            (original_bbox[0] - round_trip_bbox[0]).abs() < EPSILON,
            "minx mismatch: original={}, round_trip={}",
            original_bbox[0],
            round_trip_bbox[0]
        );
        assert!(
            (original_bbox[1] - round_trip_bbox[1]).abs() < EPSILON,
            "miny mismatch: original={}, round_trip={}",
            original_bbox[1],
            round_trip_bbox[1]
        );
        assert!(
            (original_bbox[2] - round_trip_bbox[2]).abs() < EPSILON,
            "maxx mismatch: original={}, round_trip={}",
            original_bbox[2],
            round_trip_bbox[2]
        );
        assert!(
            (original_bbox[3] - round_trip_bbox[3]).abs() < EPSILON,
            "maxy mismatch: original={}, round_trip={}",
            original_bbox[3],
            round_trip_bbox[3]
        );
    }
}
