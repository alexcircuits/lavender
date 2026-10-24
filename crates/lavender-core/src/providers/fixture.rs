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
//! | `launches.json` | Launch Library 2 `{ "results": [...] }` launch list | launches |
//! | `aisstream.jsonl` | AISStream messages, one JSON frame per line | vessels (recorded times kept) |
//! | `perimeters-wfigs.geojson` | one WFIGS current-perimeters GeoJSON query page | perimeters |
//! | `perimeters-inciweb-index.json` | InciWeb `api/map_data` array (optional) | perimeter links |
//! | `perimeters-inciweb-<slug>.html` | InciWeb incident pages (optional) | perimeter links |

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use lavender_domain::{FeedId, FeedItems, ProviderScope, VesselTrackResponse};

use super::{
    FeedProvider, FetchContext, Fetched, ProviderError, Region, adsb_lol, celestrak, cyclones, usgs,
};
use crate::config::AisStreamConfig;
use crate::providers::{aisstream, firms, launches, perimeters};
use crate::sources;
use crate::vessel_store::{Mmsi, VesselStore};

/// Largest fixture file read, matching the fetcher's ceiling for live bodies.
const MAX_FIXTURE_BYTES: u64 = 64 * 1024 * 1024;
/// How often a fixture is "refreshed" (re-read), so the stream path is exercised.
const FIXTURE_CADENCE: Duration = Duration::from_secs(30);
/// The notice every fixture snapshot carries.
pub const FIXTURE_NOTICE: &str = "Recorded fixture data, not live.";
/// Vessel replays keep the recording's own report times.
pub const VESSEL_NOTICE: &str = "Vessel positions keep their recorded report times.";
/// The notice instead of `VESSEL_NOTICE` for a replay that declares itself synthetic, as the
/// checked-in one does (`apps/web/e2e/fixtures/aisstream-provenance.md`).
pub const SYNTHETIC_VESSEL_NOTICE: &str =
    "Synthetic AIS replay: invented test ships at their recorded report times.";
/// An AIS replay declares itself synthetic with a first line
/// `{"MessageType":"LavenderFixtureProvenance","Synthetic":true,…}`; the AISStream parser
/// ignores it like any message type Lavender does not subscribe to.
pub const AIS_PROVENANCE_MESSAGE_TYPE: &str = "LavenderFixtureProvenance";
/// At most this much of a replay is read to find that first line.
const PROVENANCE_PEEK_BYTES: u64 = 4 * 1024;

#[derive(Debug)]
pub struct FixtureProvider {
    feed: FeedId,
    source_id: &'static str,
    dir: PathBuf,
    /// The vessels fixture replays into a store, so tracks can be read back.
    vessel_store: Option<Arc<VesselStore>>,
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
            (FeedId::Fires, sources::NASA_FIRMS),
            (FeedId::Launches, sources::LAUNCH_LIBRARY_2),
            (FeedId::Vessels, sources::AISSTREAM),
            (FeedId::Perimeters, sources::NIFC_WFIGS),
        ]
        .into_iter()
        .map(|(feed, source_id)| {
            let vessel_store = (feed == FeedId::Vessels).then(|| {
                Arc::new(VesselStore::new(aisstream::store_limits(&AisStreamConfig::default())))
            });
            Arc::new(Self { feed, source_id, dir: dir.to_path_buf(), vessel_store })
                as Arc<dyn FeedProvider>
        })
        .collect()
    }

    /// Replays `aisstream.jsonl` into this provider's store (idempotent: replaying the
    /// same recording again changes nothing) and returns every vessel it holds.
    async fn vessels(&self) -> Result<(FeedItems, usize), ProviderError> {
        let Some(store) = &self.vessel_store else {
            return Err(ProviderError::Unavailable("no vessel store".to_owned()));
        };
        let bytes = Self::read(&self.dir.join("aisstream.jsonl")).await?;
        let recording = String::from_utf8_lossy(&bytes);
        let counts = aisstream::replay(&recording, store);
        let vessels = store.in_area(&lavender_domain::BBox::WORLD, store.limits().max_vessels);
        Ok((FeedItems::Vessels(vessels), counts.malformed))
    }

    /// Whether the AIS replay at `path` opens with a provenance line saying it is synthetic.
    /// Only the recording itself can say so: a real capture in another fixtures directory
    /// must not be called invented.
    async fn declares_synthetic(path: &Path) -> bool {
        use tokio::io::AsyncReadExt;
        let Ok(file) = tokio::fs::File::open(path).await else { return false };
        let mut head = Vec::new();
        if file.take(PROVENANCE_PEEK_BYTES).read_to_end(&mut head).await.is_err() {
            return false;
        }
        let head = String::from_utf8_lossy(&head);
        let Some(first) = head.lines().map(str::trim).find(|line| !line.is_empty()) else {
            return false;
        };
        serde_json::from_str::<serde_json::Value>(first).is_ok_and(|frame| {
            frame["MessageType"] == AIS_PROVENANCE_MESSAGE_TYPE && frame["Synthetic"] == true
        })
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

    /// Replays a recorded WFIGS page joined with whatever recorded InciWeb index and
    /// incident pages exist; without them the perimeters simply carry no links. The
    /// answer carries the live provider's notices after the fixture notice.
    async fn perimeters(&self) -> Result<Fetched, ProviderError> {
        let wfigs = Self::read(&self.dir.join("perimeters-wfigs.geojson")).await?;
        let index = Self::read(&self.dir.join("perimeters-inciweb-index.json")).await.ok();
        let mut pages = std::collections::HashMap::new();
        if let Ok(mut entries) = tokio::fs::read_dir(&self.dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name().to_string_lossy().into_owned();
                if let Some(slug) =
                    name.strip_prefix("perimeters-inciweb-").and_then(|n| n.strip_suffix(".html"))
                    && let Ok(html) = Self::read(&entry.path()).await
                {
                    pages.insert(slug.to_owned(), html);
                }
            }
        }
        let inciweb = url::Url::parse("https://inciweb.wildfire.gov/")
            .map_err(|_| ProviderError::Unconfigured("invalid InciWeb root".to_owned()))?;
        let (items, notices) = perimeters::from_recorded(
            &wfigs,
            index.as_deref(),
            |slug| pages.get(slug).cloned(),
            &inciweb,
            jiff::Timestamp::now(),
        )?;
        let mut fetched = Fetched::fresh(FeedItems::Perimeters(items));
        fetched.notices = std::iter::once(FIXTURE_NOTICE.to_owned()).chain(notices).collect();
        Ok(fetched)
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

    /// The replayed store keeps the recording's clock, so tracks keep recorded times.
    fn vessel_track(&self, mmsi: Mmsi) -> Option<VesselTrackResponse> {
        aisstream::track_response(self.vessel_store.as_ref()?, mmsi, self.source_id)
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
            FeedId::Fires => {
                let bytes = Self::read(&self.dir.join("firms.csv")).await?;
                let (detections, dropped) = firms::parse_csv(&bytes)?;
                let cells: Vec<lavender_domain::FireCell> =
                    detections.into_iter().map(Into::into).collect();
                (FeedItems::Fires(cells), dropped)
            }
            FeedId::Launches => {
                let page = launches::parse(&Self::read(&self.dir.join("launches.json")).await?)?;
                (FeedItems::Launches(page.launches), page.dropped)
            }
            FeedId::Vessels => self.vessels().await?,
            FeedId::Perimeters => return self.perimeters().await,
            FeedId::Bikeshare => (FeedItems::Bikeshare(Vec::new()), 0),
            FeedId::Transit => (FeedItems::Transit(Vec::new()), 0),
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
        if self.feed == FeedId::Vessels {
            let synthetic = Self::declares_synthetic(&self.dir.join("aisstream.jsonl")).await;
            let notice = if synthetic { SYNTHETIC_VESSEL_NOTICE } else { VESSEL_NOTICE };
            fetched.notices.push(notice.to_owned());
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
    async fn the_browser_fixture_replays_launches_with_a_notice() {
        let dir = tempfile::tempdir().unwrap();
        let browser_fixtures =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/web/e2e/fixtures");
        std::fs::copy(browser_fixtures.join("launches.json"), dir.path().join("launches.json"))
            .unwrap();
        let answer = fetch(dir.path(), FeedId::Launches).await.unwrap();
        assert_eq!(answer.notices.first().map(String::as_str), Some(FIXTURE_NOTICE));
        assert!(answer.notices.iter().any(|n| n.contains("1 malformed record")));
        let FeedItems::Launches(launches) = answer.items else { panic!("launches") };
        assert_eq!(launches.len(), 3);
        assert!(launches.iter().all(|l| l.pad.is_some()));
    }

    #[tokio::test]
    async fn only_an_ais_replay_that_declares_itself_synthetic_is_called_synthetic() {
        let position = r#"{"MessageType":"PositionReport","MetaData":{"MMSI":230990001,"time_utc":"2026-10-15 11:50:00.000000000 +0000 UTC"},"Message":{"PositionReport":{"UserID":230990001,"Valid":true,"Latitude":60.1,"Longitude":24.9,"Sog":11.5,"Cog":88.0,"TrueHeading":89,"NavigationalStatus":0}}}"#;
        let marker = r#"{"MessageType":"LavenderFixtureProvenance","Synthetic":true}"#;
        let notices = |fetched: &Fetched| {
            (
                fetched.notices.iter().any(|n| n == VESSEL_NOTICE),
                fetched.notices.iter().any(|n| n == SYNTHETIC_VESSEL_NOTICE),
            )
        };

        // A recording of real traffic (no marker) is not called invented.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("aisstream.jsonl"), format!("{position}\n")).unwrap();
        let recorded = fetch(dir.path(), FeedId::Vessels).await.unwrap();
        assert_eq!(recorded.items.len(), 1);
        assert_eq!(recorded.notices.first().map(String::as_str), Some(FIXTURE_NOTICE));
        assert_eq!(notices(&recorded), (true, false));

        // The marker must open the file and say `Synthetic: true`.
        let dir = tempfile::tempdir().unwrap();
        let later = format!("{position}\n{marker}\n");
        std::fs::write(dir.path().join("aisstream.jsonl"), later).unwrap();
        assert_eq!(notices(&fetch(dir.path(), FeedId::Vessels).await.unwrap()), (true, false));
        let unsure = marker.replace("true", "false");
        std::fs::write(dir.path().join("aisstream.jsonl"), format!("{unsure}\n{position}\n"))
            .unwrap();
        assert_eq!(notices(&fetch(dir.path(), FeedId::Vessels).await.unwrap()), (true, false));

        std::fs::write(dir.path().join("aisstream.jsonl"), format!("\n{marker}\n{position}\n"))
            .unwrap();
        let synthetic = fetch(dir.path(), FeedId::Vessels).await.unwrap();
        assert_eq!(synthetic.items.len(), 1, "the marker is not a ship");
        assert_eq!(notices(&synthetic), (false, true));
        assert!(!synthetic.notices.iter().any(|n| n.contains("malformed")));

        // The checked-in browser replay is synthetic and says so.
        let browser = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/web/e2e/fixtures/aisstream.jsonl");
        std::fs::copy(browser, dir.path().join("aisstream.jsonl")).unwrap();
        assert_eq!(notices(&fetch(dir.path(), FeedId::Vessels).await.unwrap()), (false, true));
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

    #[tokio::test]
    async fn perimeter_replay_runs_the_real_parsers_and_verified_inciweb_join() {
        let dir = tempfile::tempdir().unwrap();
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        assert!(fetch(dir.path(), FeedId::Perimeters).await.is_err(), "no recording, no feed");
        std::fs::copy(
            fixtures.join("perimeters-wfigs.geojson"),
            dir.path().join("perimeters-wfigs.geojson"),
        )
        .unwrap();
        let bare = fetch(dir.path(), FeedId::Perimeters).await.unwrap();
        let FeedItems::Perimeters(items) = &bare.items else { panic!("perimeters") };
        assert!(items.iter().all(|p| p.inciweb_url.is_none()), "no recorded InciWeb, no links");
        for name in [
            "perimeters-inciweb-index.json",
            "perimeters-inciweb-mtbdf-sand-creek.html",
            "perimeters-inciweb-cogmf-gold-mountain-fire.html",
            "perimeters-inciweb-wanes-sinlahekin-fire.html",
        ] {
            std::fs::copy(fixtures.join(name), dir.path().join(name)).unwrap();
        }
        let answer = fetch(dir.path(), FeedId::Perimeters).await.unwrap();
        assert_eq!(answer.notices.first().map(String::as_str), Some(FIXTURE_NOTICE));
        assert!(answer.notices.iter().any(|n| n == perimeters::COVERAGE_NOTICE));
        let FeedItems::Perimeters(items) = answer.items else { panic!("perimeters") };
        assert_eq!(items.len(), 7 * perimeters::LEVELS.len());
        let sand = items.iter().find(|p| p.id == "2026-MTBDF-266319").unwrap();
        assert_eq!(
            sand.inciweb_url.as_deref(),
            Some("https://inciweb.wildfire.gov/incident-information/mtbdf-sand-creek")
        );
        // Recorded perimeter dates are kept, not replaced by the replay clock.
        assert_eq!(sand.updated_at.map(|t| t.to_string()).as_deref(), Some("2026-09-14T11:08:53Z"));
    }
}
