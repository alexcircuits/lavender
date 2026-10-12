//! Earthquakes from the USGS real-time GeoJSON summary feeds (public domain).
//! Replaces GEV's browser-direct fetch (docs/audit/upstream-layers.md §2.3).

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use lavender_domain::text::{clean, clean_opt};
use lavender_domain::{Earthquake, FeedId, FeedItems, GeoPoint, ProviderScope};
use serde::Deserialize;
use url::Url;

use super::{FeedProvider, FetchContext, Fetched, ProviderError, Region, lenient, parse_rows};
use crate::config::UsgsConfig;
use crate::fetch::{FetchOptions, HostPolicy};
use crate::sources;

/// The largest summary (`all_month`) is typically 10–20 MiB; the cap leaves headroom
/// within the fetcher's hard ceiling.
const MAX_BODY: usize = 32 * 1024 * 1024;

#[derive(Debug)]
pub struct UsgsProvider {
    config: UsgsConfig,
    policy: HostPolicy,
}

impl UsgsProvider {
    #[must_use]
    pub fn new(config: UsgsConfig) -> Self {
        let policy = HostPolicy::for_url(&config.base_url);
        Self { config, policy }
    }

    fn url(&self) -> Result<Url, ProviderError> {
        self.config
            .base_url
            .join(&format!("{}.geojson", self.config.summary))
            .map_err(|e| ProviderError::Unconfigured(format!("USGS feed URL: {e}")))
    }
}

/// The document must be an object with a `features` array; each feature is then parsed
/// on its own, so one malformed feature cannot reject the feed.
#[derive(Debug, Deserialize)]
struct Collection {
    features: Vec<serde_json::Value>,
}

// Every field is lenient: a wrong type reads as missing, and `normalise` drops the feature
// only when a required field (id, magnitude, time, coordinates) is missing.
#[derive(Debug, Deserialize)]
struct Feature {
    #[serde(default, deserialize_with = "lenient")]
    id: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    properties: Option<Properties>,
    #[serde(default, deserialize_with = "lenient")]
    geometry: Option<Geometry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Properties {
    #[serde(default, deserialize_with = "lenient")]
    mag: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    mag_type: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    place: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    time: Option<i64>,
    #[serde(default, deserialize_with = "lenient")]
    updated: Option<i64>,
    #[serde(default, deserialize_with = "lenient")]
    url: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    felt: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    alert: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    status: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    tsunami: Option<i64>,
    #[serde(default, deserialize_with = "lenient")]
    sig: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct Geometry {
    #[serde(default, deserialize_with = "lenient")]
    coordinates: Option<Vec<f64>>,
}

fn small_count(v: Option<f64>) -> Option<u32> {
    // Counts arrive as JSON numbers; anything outside u32 or non-integral is dropped.
    v.filter(|n| n.is_finite() && *n >= 0.0 && *n <= f64::from(u32::MAX) && n.fract() == 0.0).map(
        |n| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let c = n as u32;
            c
        },
    )
}

/// Converts one GeoJSON feature; `None` when a required field is missing or invalid.
fn normalise(f: Feature) -> Option<Earthquake> {
    let id = clean(f.id.as_deref()?, 64);
    if id.is_empty() {
        return None;
    }
    let p = f.properties?;
    let magnitude = p.mag.filter(|m| m.is_finite() && (-2.0..=10.0).contains(m))?;
    let coords = f.geometry?.coordinates?;
    let (&lon, &lat) = (coords.first()?, coords.get(1)?);
    let depth_km = coords.get(2).copied().filter(|d| d.is_finite()).unwrap_or(0.0);
    let position = GeoPoint::new(lat, lon).ok()?;
    let time = Timestamp::from_millisecond(p.time?).ok()?;
    let updated = p.updated.and_then(|ms| Timestamp::from_millisecond(ms).ok());
    let url = clean_opt(p.url.as_deref(), 300).filter(|u| u.starts_with("https://"));
    Some(Earthquake {
        id,
        magnitude,
        magnitude_type: clean_opt(p.mag_type.as_deref(), 8),
        depth_km,
        position,
        place: clean_opt(p.place.as_deref(), 200),
        time,
        updated,
        url,
        tsunami: p.tsunami.unwrap_or(0) == 1,
        alert: clean_opt(p.alert.as_deref(), 16)
            .filter(|a| ["green", "yellow", "orange", "red"].contains(&a.as_str())),
        felt: small_count(p.felt),
        significance: small_count(p.sig),
        status: clean_opt(p.status.as_deref(), 16),
    })
}

/// Parses a USGS summary feed. Invalid features are skipped and counted.
///
/// # Errors
/// Returns an error when the document is not a GeoJSON feature collection.
pub fn parse(body: &[u8]) -> Result<(Vec<Earthquake>, usize), ProviderError> {
    let collection: Collection =
        serde_json::from_slice(body).map_err(|e| ProviderError::parse("USGS", &e))?;
    let total = collection.features.len();
    let (features, _) = parse_rows::<Feature>(collection.features);
    let mut quakes: Vec<Earthquake> = features.into_iter().filter_map(normalise).collect();
    quakes.sort_by_key(|q| std::cmp::Reverse(q.time));
    let dropped = total - quakes.len();
    Ok((quakes, dropped))
}

#[async_trait]
impl FeedProvider for UsgsProvider {
    fn feed(&self) -> FeedId {
        FeedId::Earthquakes
    }
    fn source_id(&self) -> &'static str {
        sources::USGS
    }
    fn cadence(&self) -> Duration {
        Duration::from_secs(self.config.cadence_s)
    }
    fn scope(&self) -> ProviderScope {
        ProviderScope::Global
    }
    fn enabled(&self) -> bool {
        self.config.enabled
    }

    async fn fetch(
        &self,
        ctx: &FetchContext<'_>,
        _region: Region,
    ) -> Result<Fetched, ProviderError> {
        let opts = FetchOptions {
            max_body_bytes: Some(MAX_BODY),
            cancel: Some(ctx.cancel.clone()),
            ..FetchOptions::default()
        };
        let response = ctx.fetcher.get(&self.url()?, &self.policy, &opts).await?;
        let (quakes, dropped) = parse(&response.body)?;
        if dropped > 0 {
            tracing::warn!(dropped, "USGS features skipped during validation");
        }
        Ok(Fetched::fresh(FeedItems::Earthquakes(quakes)).with_dropped("USGS", dropped))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_collection_is_a_valid_empty_result() {
        let (quakes, dropped) = parse(br#"{"type":"FeatureCollection","features":[]}"#).unwrap();
        assert_eq!(quakes, Vec::<Earthquake>::new());
        assert_eq!(dropped, 0);
    }

    #[test]
    fn malformed_rows_are_dropped_and_counted() {
        let body = br#"{"features":[
          {"id":"ok1","properties":{"mag":3.1,"time":1791754362000,"place":"  12 km N of\nSomewhere ","url":"https://earthquake.usgs.gov/x","alert":"green","felt":4,"tsunami":1},"geometry":{"coordinates":[24.9,60.2,12.5]}},
          {"id":"no-mag","properties":{"time":1791754362000},"geometry":{"coordinates":[1,1,1]}},
          {"id":"bad-lat","properties":{"mag":2.0,"time":1791754362000},"geometry":{"coordinates":[1,95,1]}},
          {"id":"no-geom","properties":{"mag":2.0,"time":1791754362000},"geometry":null},
          {"properties":{"mag":2.0,"time":1791754362000},"geometry":{"coordinates":[1,1]}},
          {"id":"bad-url","properties":{"mag":2.5,"time":1791754362000,"url":"javascript:alert(1)","alert":"purple"},"geometry":{"coordinates":[1,1]}}
        ]}"#;
        let (quakes, dropped) = parse(body).unwrap();
        assert_eq!(quakes.len(), 2);
        assert_eq!(dropped, 4);
        let ok = quakes.iter().find(|q| q.id == "ok1").unwrap();
        assert_eq!(ok.place.as_deref(), Some("12 km N of Somewhere"));
        assert!((ok.depth_km - 12.5).abs() < 1e-9);
        assert!(ok.tsunami && ok.felt == Some(4) && ok.alert.as_deref() == Some("green"));
        let bad = quakes.iter().find(|q| q.id == "bad-url").unwrap();
        assert_eq!(bad.url, None, "non-https links are dropped");
        assert_eq!(bad.alert, None, "unknown alert levels are dropped");
        assert!((bad.depth_km).abs() < 1e-9, "missing depth defaults to 0");
    }

    #[test]
    fn non_geojson_is_a_parse_error() {
        assert!(parse(b"[]").is_err());
        assert!(parse(b"<html/>").is_err());
    }

    #[test]
    fn wrong_typed_fields_drop_only_their_row() {
        // Review finding 10: one wrong-typed field used to reject the whole feed.
        let body = br#"{"features":[
          {"id":"ok","properties":{"mag":3.1,"time":1791754362000,"felt":"many","tsunami":"no"},"geometry":{"coordinates":[24.9,60.2,12.5]}},
          {"id":"null-props","properties":null,"geometry":{"coordinates":[1,1]}},
          {"id":"string-mag","properties":{"mag":"3.1","time":1791754362000},"geometry":{"coordinates":[1,1]}},
          {"id":"null-coord","properties":{"mag":2.0,"time":1791754362000},"geometry":{"coordinates":[null,1]}},
          "not an object",
          42
        ]}"#;
        let (quakes, dropped) = parse(body).unwrap();
        assert_eq!(quakes.len(), 1);
        assert_eq!(quakes[0].id, "ok");
        assert_eq!(quakes[0].felt, None, "a wrong-typed optional field reads as missing");
        assert!(!quakes[0].tsunami);
        assert_eq!(dropped, 5);
    }

    #[test]
    fn parse_errors_do_not_echo_large_upstream_content() {
        let body = format!(r#"{{"features":"{}"}}"#, "A".repeat(2 * 1024 * 1024));
        let err = parse(body.as_bytes()).unwrap_err().to_string();
        assert!(err.len() < 300, "error is {} bytes", err.len());
        assert!(err.starts_with("USGS sent data in an unexpected shape"), "{err}");
    }
}
