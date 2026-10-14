//! Validated geographic points and bounding boxes, including views across the antimeridian.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::DomainError;

/// A WGS84 position. Latitude and longitude are validated; altitude is metres above the
/// ellipsoid when known.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", try_from = "RawGeoPoint")]
#[ts(export)]
pub struct GeoPoint {
    pub lat: f64,
    pub lon: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub alt_m: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawGeoPoint {
    lat: f64,
    lon: f64,
    alt_m: Option<f64>,
}

impl TryFrom<RawGeoPoint> for GeoPoint {
    type Error = DomainError;

    fn try_from(raw: RawGeoPoint) -> Result<Self, Self::Error> {
        Self::new(raw.lat, raw.lon)?.with_altitude(raw.alt_m)
    }
}

impl GeoPoint {
    /// Validates and builds a position.
    ///
    /// # Errors
    /// Returns an error when either coordinate is not finite or out of range.
    pub fn new(lat: f64, lon: f64) -> Result<Self, DomainError> {
        check_lat(lat)?;
        check_lon(lon)?;
        Ok(Self { lat, lon, alt_m: None })
    }

    /// Attaches an altitude in metres.
    ///
    /// # Errors
    /// Returns an error when the altitude is not finite.
    pub fn with_altitude(mut self, alt_m: Option<f64>) -> Result<Self, DomainError> {
        if let Some(alt) = alt_m
            && !alt.is_finite()
        {
            return Err(DomainError::NotFinite { field: "altM" });
        }
        self.alt_m = alt_m;
        Ok(self)
    }
}

fn check_lat(lat: f64) -> Result<(), DomainError> {
    if !lat.is_finite() {
        return Err(DomainError::NotFinite { field: "lat" });
    }
    if !(-90.0..=90.0).contains(&lat) {
        return Err(DomainError::Latitude(lat));
    }
    Ok(())
}

fn check_lon(lon: f64) -> Result<(), DomainError> {
    if !lon.is_finite() {
        return Err(DomainError::NotFinite { field: "lon" });
    }
    if !(-180.0..=180.0).contains(&lon) {
        return Err(DomainError::Longitude(lon));
    }
    Ok(())
}

/// A geographic bounding box in degrees. When `west > east` the box crosses the
/// antimeridian (for example `west = 170, east = -170` spans 20° of longitude).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", try_from = "RawBBox")]
#[ts(export)]
pub struct BBox {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

#[derive(Deserialize)]
struct RawBBox {
    west: f64,
    south: f64,
    east: f64,
    north: f64,
}

impl TryFrom<RawBBox> for BBox {
    type Error = DomainError;

    fn try_from(raw: RawBBox) -> Result<Self, Self::Error> {
        Self::new(raw.west, raw.south, raw.east, raw.north)
    }
}

impl BBox {
    /// The whole world.
    pub const WORLD: Self = Self { west: -180.0, south: -90.0, east: 180.0, north: 90.0 };

    /// Validates and builds a box.
    ///
    /// # Errors
    /// Returns an error for non-finite or out-of-range edges, or when `south > north`.
    pub fn new(west: f64, south: f64, east: f64, north: f64) -> Result<Self, DomainError> {
        check_lon(west)?;
        check_lon(east)?;
        check_lat(south)?;
        check_lat(north)?;
        if south > north {
            return Err(DomainError::InvertedBBox { south, north });
        }
        Ok(Self { west, south, east, north })
    }

    /// Parses `"west,south,east,north"`, the form used in query strings.
    ///
    /// # Errors
    /// Returns an error when there are not exactly four numbers or they are out of range.
    pub fn parse(input: &str) -> Result<Self, DomainError> {
        let parts: Vec<f64> = input
            .split(',')
            .map(|p| p.trim().parse::<f64>())
            .collect::<Result<_, _>>()
            .map_err(|e| DomainError::Parse { what: "bbox", reason: e.to_string() })?;
        let [west, south, east, north] = parts[..] else {
            return Err(DomainError::Parse {
                what: "bbox",
                reason: format!("expected 4 numbers, got {}", parts.len()),
            });
        };
        Self::new(west, south, east, north)
    }

    /// Whether the box spans the antimeridian.
    #[must_use]
    pub fn crosses_antimeridian(&self) -> bool {
        self.west > self.east
    }

    /// Longitudinal span in degrees (0..=360).
    #[must_use]
    pub fn lon_span(&self) -> f64 {
        if self.crosses_antimeridian() {
            360.0 - (self.west - self.east)
        } else {
            self.east - self.west
        }
    }

    /// Latitudinal span in degrees.
    #[must_use]
    pub fn lat_span(&self) -> f64 {
        self.north - self.south
    }

    /// Whether `point` lies inside the box (edges inclusive).
    #[must_use]
    pub fn contains(&self, point: &GeoPoint) -> bool {
        if point.lat < self.south || point.lat > self.north {
            return false;
        }
        if self.crosses_antimeridian() {
            point.lon >= self.west || point.lon <= self.east
        } else {
            point.lon >= self.west && point.lon <= self.east
        }
    }

    /// Whether two boxes overlap (edges inclusive), antimeridian-aware.
    #[must_use]
    pub fn intersects(&self, other: &Self) -> bool {
        if self.north < other.south || other.north < self.south {
            return false;
        }
        self.lon_ranges()
            .iter()
            .any(|(a0, a1)| other.lon_ranges().iter().any(|(b0, b1)| a0 <= b1 && b0 <= a1))
    }

    /// The box as one or two non-crossing longitude ranges.
    #[must_use]
    pub fn lon_ranges(&self) -> Vec<(f64, f64)> {
        if self.crosses_antimeridian() {
            vec![(self.west, 180.0), (-180.0, self.east)]
        } else {
            vec![(self.west, self.east)]
        }
    }

    /// The centre point, antimeridian-aware.
    #[must_use]
    pub fn center(&self) -> GeoPoint {
        let lat = f64::midpoint(self.south, self.north);
        let mut lon = self.west + self.lon_span() / 2.0;
        if lon > 180.0 {
            lon -= 360.0;
        }
        GeoPoint { lat, lon, alt_m: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn rejects_out_of_range_points() {
        assert!(GeoPoint::new(91.0, 0.0).is_err());
        assert!(GeoPoint::new(0.0, 180.5).is_err());
        assert!(GeoPoint::new(f64::NAN, 0.0).is_err());
        assert!(GeoPoint::new(60.17, 24.94).is_ok());
    }

    #[test]
    fn deserialising_validates() {
        let bad = serde_json::from_str::<GeoPoint>(r#"{"lat":120,"lon":0}"#);
        assert!(bad.is_err());
        let ok: GeoPoint = serde_json::from_str(r#"{"lat":1.5,"lon":2.5,"altM":100}"#).unwrap();
        assert_eq!(ok.alt_m, Some(100.0));
        let bad_box = serde_json::from_str::<BBox>(r#"{"west":0,"south":10,"east":5,"north":5}"#);
        assert!(bad_box.is_err());
    }

    #[test]
    fn serialises_camel_case_without_empty_altitude() {
        let p = GeoPoint::new(1.0, 2.0).unwrap();
        assert_eq!(serde_json::to_string(&p).unwrap(), r#"{"lat":1.0,"lon":2.0}"#);
    }

    #[test]
    fn parses_query_string_bbox() {
        let b = BBox::parse("-10, 35, 30, 60").unwrap();
        assert!((b.lon_span() - 40.0).abs() < 1e-9);
        assert!(BBox::parse("1,2,3").is_err());
        assert!(BBox::parse("a,b,c,d").is_err());
    }

    #[test]
    fn antimeridian_box_contains_both_sides() {
        let b = BBox::new(170.0, -10.0, -170.0, 10.0).unwrap();
        assert!(b.crosses_antimeridian());
        assert!((b.lon_span() - 20.0).abs() < 1e-9);
        assert!(b.contains(&GeoPoint::new(0.0, 175.0).unwrap()));
        assert!(b.contains(&GeoPoint::new(0.0, -175.0).unwrap()));
        assert!(!b.contains(&GeoPoint::new(0.0, 0.0).unwrap()));
        let c = b.center();
        assert!((c.lon.abs() - 180.0).abs() < 1e-9);
    }

    #[test]
    fn intersects_across_antimeridian() {
        let a = BBox::new(170.0, -10.0, -170.0, 10.0).unwrap();
        let b = BBox::new(-175.0, -5.0, -160.0, 5.0).unwrap();
        let c = BBox::new(0.0, -5.0, 10.0, 5.0).unwrap();
        assert!(a.intersects(&b));
        assert!(!a.intersects(&c));
        assert!(BBox::WORLD.intersects(&a));
    }

    proptest! {
        #[test]
        fn contains_center(w in -180.0f64..180.0, e in -180.0f64..180.0, s in -90.0f64..0.0, n in 0.0f64..90.0) {
            let b = BBox::new(w, s, e, n).unwrap();
            prop_assert!(b.contains(&b.center()));
        }

        #[test]
        fn span_is_within_world(w in -180.0f64..180.0, e in -180.0f64..180.0) {
            let b = BBox::new(w, -1.0, e, 1.0).unwrap();
            prop_assert!(b.lon_span() >= 0.0 && b.lon_span() <= 360.0);
        }
    }
}
