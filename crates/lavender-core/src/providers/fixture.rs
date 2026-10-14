//! Recorded upstream documents replayed from disk instead of the network: end-to-end tests
//! and offline demonstrations run the real parsers, hub, API and interface without
//! touching a live source. Never the default; adapters enable it explicitly and every
//! snapshot carries a notice saying the data is recorded.
//!
//! Directory layout (each file optional; a missing file makes that feed unavailable):
//!
//! | File | Format | Feed |
//! |---|---|---|
//! | `usgs.geojson` | USGS GeoJSON summary feed | earthquakes |
//! | `adsb-lol.json` | adsb.lol v2 `{ "ac": [...] }` | aircraft |
//! | `celestrak-<group>.json` | CelesTrak OMM JSON array | satellites (`group` from the name) |

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use lavender_domain::{FeedId, FeedItems, ProviderScope};

use super::{
    FeedProvider, FetchContext, Fetched, ProviderError, Region, adsb_lol, celestrak, cyclones, usgs,
};
use crate::sources;

/// Largest fixture file read, matching the fetcher's ceiling for live bodies.
const MAX_FIXTURE_BYTES: u64 = 64 * 1024 * 1024;
/// How often a fixture is "refreshed" (re-read), so the stream path is exercised.
const FIXTURE_CADENCE: Duration = Duration::from_secs(30);
/// The notice every fixture snapshot carries.
pub const FIXTURE_NOTICE: &str = "Recorded fixture data, not live.";

#[derive(Debug)]
pub struct FixtureProvider {
    feed: FeedId,
    source_id: &'static str,
    dir: PathBuf,
}

impl FixtureProvider {
    /// One fixture provider per feed Lavender knows, reading from `dir`.
    #[must_use]
    pub fn all(dir: &Path) -> Vec<Arc<dyn FeedProvider>> {
        [
            (FeedId::Earthquakes, sources::USGS),
            (FeedId::Aircraft, sources::ADSB_LOL),
            (FeedId::Satellites, sources::CELESTRAK),
            (FeedId::Cyclones, sources::NOAA_CYCLONES),
        ]
        .into_iter()
        .map(|(feed, source_id)| {
            Arc::new(Self { feed, source_id, dir: dir.to_path_buf() }) as Arc<dyn FeedProvider>
        })
        .collect()
    }

    async fn read(path: &Path) -> Result<Vec<u8>, ProviderError> {
        let unavailable =
            |what: &str| ProviderError::Unavailable(format!("fixture {}: {what}", path.display()));
        let meta = tokio::fs::metadata(path).await.map_err(|_| unavailable("missing"))?;
        if meta.len() > MAX_FIXTURE_BYTES {
            return Err(unavailable("too large"));
        }
        tokio::fs::read(path).await.map_err(|_| unavailable("unreadable"))
    }

    async fn cyclones(&self) -> Result<(FeedItems, usize), ProviderError> {
        let status = Self::read(&self.dir.join("cyclones-status.json")).await?;
        let document: serde_json::Value = serde_json::from_slice(&status)
            .map_err(|_| ProviderError::parse("Cyclone fixture", &"invalid status document"))?;
        // Replay keeps the advisory's recorded timestamps rather than claiming that the
        // old forecast was issued now. Production never uses this fixture clock.
        let fixture_clock = document["activeStorms"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s["lastUpdate"].as_str()?.parse::<jiff::Timestamp>().ok())
            .max()
            .unwrap_or_else(jiff::Timestamp::now);
        let mut storms = cyclones::parse_status(&status, fixture_clock)?;
        if !storms.is_empty() {
            let points = Self::read(&self.dir.join("cyclones-points.json")).await?;
            let track = Self::read(&self.dir.join("cyclones-track.json")).await?;
            let cone = Self::read(&self.dir.join("cyclones-cone.json")).await?;
            cyclones::attach_geometry(&mut storms, [&points, &track, &cone])?;
        }
        Ok((FeedItems::Cyclones(storms), 0))
    }

    async fn satellites(&self) -> Result<(FeedItems, usize), ProviderError> {
        let mut entries = tokio::fs::read_dir(&self.dir)
            .await
            .map_err(|_| ProviderError::Unavailable("fixture directory unreadable".to_owned()))?;
        let mut files = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(group) =
                name.strip_prefix("celestrak-").and_then(|n| n.strip_suffix(".json"))
            {
                files.push((group.to_owned(), entry.path()));
            }
        }
        if files.is_empty() {
            return Err(ProviderError::Unavailable(
                "no celestrak-<group>.json fixtures".to_owned(),
            ));
        }
        files.sort();
        let (mut all, mut dropped) = (Vec::new(), 0);
        for (group, path) in files {
            let (sets, skipped) = celestrak::parse(&Self::read(&path).await?, &group)?;
            all.extend(sets);
            dropped += skipped;
        }
        Ok((FeedItems::Satellites(all), dropped))
    }
}

#[async_trait]
impl FeedProvider for FixtureProvider {
    fn feed(&self) -> FeedId {
        self.feed
    }

    fn source_id(&self) -> &'static str {
        self.source_id
    }

    fn cadence(&self) -> Duration {
        FIXTURE_CADENCE
    }

    fn scope(&self) -> ProviderScope {
        ProviderScope::Global
    }

    fn enabled(&self) -> bool {
        true
    }

    async fn fetch(
        &self,
        _ctx: &FetchContext<'_>,
        _region: Region,
    ) -> Result<Fetched, ProviderError> {
        let (items, dropped) = match self.feed {
            FeedId::Earthquakes => {
                let (q, d) = usgs::parse(&Self::read(&self.dir.join("usgs.geojson")).await?)?;
                (FeedItems::Earthquakes(q), d)
            }
            FeedId::Aircraft => {
                let (a, d) =
                    adsb_lol::parse(&Self::read(&self.dir.join("adsb-lol.json")).await?, false)?;
                (FeedItems::Aircraft(a), d)
            }
            FeedId::Satellites => self.satellites().await?,
            FeedId::Cyclones => self.cyclones().await?,
        };
        let mut fetched = Fetched::fresh(items).with_dropped("Fixture", dropped);
        if self.feed == FeedId::Aircraft {
            // Fixture mode replays positions on the current clock; this is not their
            // historical observation date, and never runs for production providers.
            fetched.as_of = Some(jiff::Timestamp::now());
            fetched
                .notices
                .push("Aircraft history timestamps use the fixture replay clock.".to_owned());
        }
        if self.feed == FeedId::Cyclones {
            fetched.notices.push(cyclones::COVERAGE_NOTICE.to_owned());
            fetched.notices.push(cyclones::CONE_NOTICE.to_owned());
        }
        fetched.notices.insert(0, FIXTURE_NOTICE.to_owned());
        Ok(fetched)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::Fetcher;
    use crate::secrets::MemorySecretStore;
    use tokio_util::sync::CancellationToken;

    async fn fetch(dir: &Path, feed: FeedId) -> Result<Fetched, ProviderError> {
        let provider = FixtureProvider::all(dir).into_iter().find(|p| p.feed() == feed).unwrap();
        let fetcher = Fetcher::new(crate::config::FetchConfig::default()).unwrap();
        let secrets = MemorySecretStore::default();
        let ctx = FetchContext {
            fetcher: &fetcher,
            secrets: &secrets,
            cancel: CancellationToken::new(),
            cache_dir: None,
        };
        provider.fetch(&ctx, Region::Global).await
    }

    #[tokio::test]
    async fn replays_recorded_documents_with_a_notice() {
        let dir = tempfile::tempdir().unwrap();
        let fixtures =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/usgs-2.5_day.geojson");
        std::fs::copy(fixtures, dir.path().join("usgs.geojson")).unwrap();
        std::fs::write(dir.path().join("adsb-lol.json"), r#"{"ac":[{"hex":"4ca1b2","flight":"RYR12AB","lat":61.0,"lon":24.0,"alt_baro":35000},{"hex":"bad","lat":"x"}]}"#).unwrap();
        std::fs::write(
            dir.path().join("celestrak-stations.json"),
            r#"[{"OBJECT_NAME":"TEST STATION","OBJECT_ID":"1998-067A","EPOCH":"2026-10-10T12:00:00.000000","MEAN_MOTION":15.5,"ECCENTRICITY":0.0007,"INCLINATION":51.64,"RA_OF_ASC_NODE":247.46,"ARG_OF_PERICENTER":130.53,"MEAN_ANOMALY":325.03,"NORAD_CAT_ID":25544,"BSTAR":0.0003,"MEAN_MOTION_DOT":0.00016,"MEAN_MOTION_DDOT":0}]"#,
        )
        .unwrap();

        let quakes = fetch(dir.path(), FeedId::Earthquakes).await.unwrap();
        assert!(quakes.items.len() > 40);
        assert_eq!(quakes.notices.first().map(String::as_str), Some(FIXTURE_NOTICE));

        let aircraft = fetch(dir.path(), FeedId::Aircraft).await.unwrap();
        assert_eq!(aircraft.items.len(), 1);
        assert!(aircraft.notices.iter().any(|n| n.contains("1 malformed record")));

        let sats = fetch(dir.path(), FeedId::Satellites).await.unwrap();
        let FeedItems::Satellites(element_sets) = sats.items else { panic!("satellites") };
        assert_eq!(element_sets[0].group, "stations");
    }

    #[tokio::test]
    async fn a_missing_fixture_makes_only_that_feed_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        let err = fetch(dir.path(), FeedId::Aircraft).await.unwrap_err();
        assert!(matches!(err, ProviderError::Unavailable(_)), "{err}");
        assert!(fetch(dir.path(), FeedId::Satellites).await.is_err());
    }

    #[tokio::test]
    async fn cyclone_replay_preserves_advisory_dates_and_explains_coverage() {
        let dir = tempfile::tempdir().unwrap();
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        for name in ["status", "points", "track", "cone"] {
            let file = format!("cyclones-{name}.json");
            std::fs::copy(fixtures.join(&file), dir.path().join(&file)).unwrap();
        }
        let answer = fetch(dir.path(), FeedId::Cyclones).await.unwrap();
        assert_eq!(answer.notices.first().map(String::as_str), Some(FIXTURE_NOTICE));
        assert!(answer.notices.iter().any(|n| n.contains("not worldwide")));
        let FeedItems::Cyclones(storms) = answer.items else { panic!("cyclones") };
        assert_eq!(storms[0].position_at.to_string(), "2026-10-14T00:00:00Z");
        assert_eq!(storms[0].geometry_status, lavender_domain::CycloneGeometryStatus::Current);
    }
}
