//! Pure map geometry for the inspect Map tab: geographic gate and the
//! embedded world coastline.

use std::sync::OnceLock;

use crate::inspect::model::{CrsInfo, ExtentInfo};

/// EPSG codes we treat as geographic (lon/lat): WGS84 2D, WGS84 3D, ETRS89.
pub const GEOGRAPHIC_EPSG: [i32; 3] = [4326, 4979, 4258];

/// Embedded, decimated world coastline (`lon,lat` per line, `#` comments).
const COASTLINE_CSV: &str = include_str!("../../assets/coastline.csv");

fn extent_in_lonlat(extent: &ExtentInfo) -> bool {
    extent.min[0] >= -180.0
        && extent.max[0] <= 180.0
        && extent.min[1] >= -90.0
        && extent.max[1] <= 90.0
}

/// Geographic when the CRS is a known geographic EPSG *and* the extent lies in
/// lon/lat range; with no CRS, fall back to the extent-bounds check alone.
pub fn is_geographic(crs: Option<&CrsInfo>, extent: &ExtentInfo) -> bool {
    match crs {
        Some(c) => GEOGRAPHIC_EPSG.contains(&c.code) && extent_in_lonlat(extent),
        None => extent_in_lonlat(extent),
    }
}

/// Parse the embedded coastline once, skipping `#` comment and blank lines.
pub fn coastline_points() -> &'static [(f64, f64)] {
    static POINTS: OnceLock<Vec<(f64, f64)>> = OnceLock::new();
    POINTS
        .get_or_init(|| {
            COASTLINE_CSV
                .lines()
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .filter_map(|l| {
                    let (lon, lat) = l.split_once(',')?;
                    Some((lon.trim().parse().ok()?, lat.trim().parse().ok()?))
                })
                .collect()
        })
        .as_slice()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::model::{CrsInfo, ExtentInfo};

    fn geo_extent() -> ExtentInfo {
        ExtentInfo {
            min: [4.0, 52.0, 0.0],
            max: [5.0, 53.0, 10.0],
        }
    }
    fn projected_extent() -> ExtentInfo {
        // Typical EPSG:28992 (metres) values, far outside lon/lat range.
        ExtentInfo {
            min: [84000.0, 447000.0, 0.0],
            max: [85000.0, 448000.0, 10.0],
        }
    }

    #[test]
    fn geographic_epsg_with_lonlat_extent_is_geographic() {
        let crs = CrsInfo {
            authority: Some("EPSG".into()),
            code: 4326,
            version: 0,
            code_string: None,
        };
        assert!(is_geographic(Some(&crs), &geo_extent()));
    }

    #[test]
    fn projected_epsg_is_not_geographic() {
        let crs = CrsInfo {
            authority: Some("EPSG".into()),
            code: 28992,
            version: 0,
            code_string: None,
        };
        assert!(!is_geographic(Some(&crs), &projected_extent()));
    }

    #[test]
    fn no_crs_falls_back_to_extent_bounds() {
        assert!(is_geographic(None, &geo_extent()));
        assert!(!is_geographic(None, &projected_extent()));
    }

    #[test]
    fn coastline_points_are_within_lonlat_bounds() {
        let pts = coastline_points();
        assert!(
            pts.len() >= 1000,
            "expected a substantial coastline, got {}",
            pts.len()
        );
        for &(lon, lat) in pts {
            assert!((-180.0..=180.0).contains(&lon), "lon out of range: {lon}");
            assert!((-90.0..=90.0).contains(&lat), "lat out of range: {lat}");
        }
    }
}
