//! Layers, sources and the licence gate (ADR 0008).

use std::collections::HashSet;

use lavender_domain::{
    Availability, Catalog, CommercialUse, FeedId, LayerDescriptor, LayerFamily, LicenceMode,
    Privacy, ProviderScope, SourceInfo,
};

use crate::secrets::SecretStore;
use crate::sources::{self, SourceDef};

/// How a feed is presented, independent of which provider implements it.
#[derive(Debug, Clone)]
pub struct LayerDef {
    pub feed: FeedId,
    pub name: &'static str,
    pub description: &'static str,
    pub family: LayerFamily,
    pub sources: Vec<&'static str>,
    pub default_on: bool,
}

#[must_use]
pub fn layers() -> Vec<LayerDef> {
    vec![
        LayerDef {
            feed: FeedId::Aircraft,
            name: "Aircraft",
            description: "Live ADS-B and MLAT positions, civil and military.",
            family: LayerFamily::Air,
            sources: vec![sources::ADSB_LOL],
            default_on: false,
        },
        LayerDef {
            feed: FeedId::Earthquakes,
            name: "Earthquakes",
            description: "Seismic events from the last day, magnitude 2.5 and above.",
            family: LayerFamily::Hazards,
            sources: vec![sources::USGS],
            default_on: true,
        },
        LayerDef {
            feed: FeedId::Satellites,
            name: "Satellites",
            description: "Space stations, bright, navigation and recently launched satellites, propagated live.",
            family: LayerFamily::Space,
            sources: vec![sources::CELESTRAK],
            default_on: false,
        },
        LayerDef {
            feed: FeedId::Cyclones,
            name: "Tropical cyclones",
            description: "NHC/CPHC advisory positions, forecast tracks and uncertainty cones for the Atlantic and eastern/central North Pacific.",
            family: LayerFamily::Hazards,
            sources: vec![sources::NOAA_CYCLONES],
            default_on: false,
        },
        LayerDef {
            feed: FeedId::Launches,
            name: "Rocket launches",
            description: "Recent and upcoming rocket launches (30 days either side): launch sites, status and the tracked satellites each launch carried.",
            family: LayerFamily::Space,
            sources: vec![sources::LAUNCH_LIBRARY_2],
            default_on: false,
        },
        LayerDef {
            feed: FeedId::Fires,
            name: "Wildfires",
            description: "NASA FIRMS thermal anomalies and active fire detections (MODIS/VIIRS).",
            family: LayerFamily::Hazards,
            sources: vec![sources::NASA_FIRMS],
            default_on: false,
        },
        LayerDef {
            feed: FeedId::Vessels,
            name: "Vessels",
            description: "Live AIS ship positions relayed by AISStream receivers, with recent tracks.",
            family: LayerFamily::Maritime,
            sources: vec![sources::AISSTREAM],
            default_on: false,
        },
        LayerDef {
            feed: FeedId::Perimeters,
            name: "Wildfire perimeters",
            description: "NIFC WFIGS outlines of current US wildland fires, simplified for the view, with containment and InciWeb incident links.",
            family: LayerFamily::Hazards,
            sources: vec![sources::NIFC_WFIGS, sources::INCIWEB],
            default_on: false,
        },
        LayerDef {
            feed: FeedId::Bikeshare,
            name: "Bikeshare",
            description: "Live station availability and dock capacity for municipal bike share networks.",
            family: LayerFamily::Mobility,
            sources: vec![sources::GBFS],
            default_on: false,
        },
        LayerDef {
            feed: FeedId::Transit,
            name: "Public transit",
            description: "Live public transit vehicle positions and historical routes (GTFS-RT).",
            family: LayerFamily::Mobility,
            sources: vec![sources::GTFS_RT],
            default_on: false,
        },
    ]
}

/// The registry: what exists, what is enabled, and why.
#[derive(Debug, Clone)]
pub struct Registry {
    sources: Vec<SourceDef>,
    layers: Vec<LayerDef>,
    mode: LicenceMode,
    acknowledged: HashSet<String>,
}

impl Registry {
    #[must_use]
    pub fn new(mode: LicenceMode, acknowledged: &[String]) -> Self {
        Self {
            sources: sources::all(),
            layers: layers(),
            mode,
            acknowledged: acknowledged.iter().cloned().collect(),
        }
    }

    /// A registry over custom sources (tests and specialised deployments).
    #[must_use]
    pub fn with_sources(
        sources: Vec<SourceDef>,
        mode: LicenceMode,
        acknowledged: &[String],
    ) -> Self {
        Self {
            sources,
            layers: layers(),
            mode,
            acknowledged: acknowledged.iter().cloned().collect(),
        }
    }

    #[must_use]
    pub fn mode(&self) -> LicenceMode {
        self.mode
    }

    #[must_use]
    pub fn source(&self, id: &str) -> Option<&SourceDef> {
        self.sources.iter().find(|s| s.info.id == id)
    }

    #[must_use]
    pub fn sources(&self) -> Vec<SourceInfo> {
        self.sources.iter().map(|s| s.info.clone()).collect()
    }

    #[must_use]
    pub fn layer(&self, feed: FeedId) -> Option<&LayerDef> {
        self.layers.iter().find(|l| l.feed == feed)
    }

    /// Whether the licence policy permits a source. Sensitive sources are never enabled
    /// implicitly; they must be acknowledged in every mode.
    #[must_use]
    pub fn licence_permits(&self, id: &str) -> bool {
        let Some(def) = self.source(id) else { return false };
        if def.info.privacy == Privacy::Sensitive && !self.acknowledged.contains(id) {
            return false;
        }
        match self.mode {
            LicenceMode::Evaluation => true,
            LicenceMode::Commercial => {
                def.info.commercial == CommercialUse::Allowed || self.acknowledged.contains(id)
            }
        }
    }

    /// Whether a feed can run, and if not, why.
    #[must_use]
    pub fn availability(
        &self,
        feed: FeedId,
        secrets: &dyn SecretStore,
        provider_enabled: bool,
    ) -> Availability {
        let Some(layer) = self.layer(feed) else {
            return Availability::Unconfigured { missing: vec!["provider".to_owned()] };
        };
        if !provider_enabled {
            return Availability::Unconfigured {
                missing: vec!["provider disabled in configuration".to_owned()],
            };
        }
        let permitted: Vec<&SourceDef> = layer
            .sources
            .iter()
            .filter_map(|id| self.source(id))
            .filter(|s| self.licence_permits(&s.info.id))
            .collect();
        if permitted.is_empty() {
            return Availability::DisabledByLicence;
        }
        let missing: Vec<String> = permitted
            .iter()
            .flat_map(|s| s.required_secrets.iter())
            .filter(|k| !secrets.is_set(**k))
            .map(|k| k.label().to_owned())
            .collect();
        if permitted.iter().any(|s| s.required_secrets.iter().all(|k| secrets.is_set(*k))) {
            Availability::Available
        } else {
            Availability::Unconfigured { missing }
        }
    }

    /// Builds the catalog; `meta` supplies each feed's cadence, scope and whether its
    /// provider is enabled.
    #[must_use]
    pub fn catalog(
        &self,
        secrets: &dyn SecretStore,
        meta: impl Fn(FeedId) -> Option<(u32, ProviderScope, bool)>,
    ) -> Catalog {
        let layers = self
            .layers
            .iter()
            .filter_map(|l| {
                let (cadence_s, scope, enabled) = meta(l.feed)?;
                Some(LayerDescriptor {
                    feed: l.feed,
                    name: l.name.to_owned(),
                    description: l.description.to_owned(),
                    family: l.family,
                    source_ids: l.sources.iter().map(|s| (*s).to_owned()).collect(),
                    availability: self.availability(l.feed, secrets, enabled),
                    cadence_s,
                    scope,
                    default_on: l.default_on,
                })
            })
            .collect();
        let imagery =
            sources::imagery().into_iter().filter(|i| self.licence_permits(&i.source_id)).collect();
        Catalog { layers, sources: self.sources(), imagery, licence_mode: self.mode }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::MemorySecretStore;

    #[test]
    fn evaluation_mode_permits_everything_registered() {
        let r = Registry::new(LicenceMode::Evaluation, &[]);
        assert!(r.licence_permits(sources::CELESTRAK));
        assert!(!r.licence_permits("no-such-source"));
    }

    #[test]
    fn commercial_mode_requires_allowed_or_acknowledged() {
        let r = Registry::new(LicenceMode::Commercial, &[]);
        assert!(r.licence_permits(sources::USGS));
        assert!(!r.licence_permits(sources::CELESTRAK));
        let secrets = MemorySecretStore::default();
        assert_eq!(
            r.availability(FeedId::Satellites, &secrets, true),
            Availability::DisabledByLicence
        );
        let r = Registry::new(LicenceMode::Commercial, &[sources::CELESTRAK.to_owned()]);
        assert_eq!(r.availability(FeedId::Satellites, &secrets, true), Availability::Available);
    }

    #[test]
    fn sensitive_sources_are_off_until_acknowledged_in_every_mode() {
        let mut camera = sources::all().remove(0);
        camera.info.id = "public-cameras".into();
        camera.info.privacy = Privacy::Sensitive;
        for mode in [LicenceMode::Evaluation, LicenceMode::Commercial] {
            let r = Registry::with_sources(vec![camera.clone()], mode, &[]);
            assert!(!r.licence_permits("public-cameras"), "{mode:?}");
            let r =
                Registry::with_sources(vec![camera.clone()], mode, &["public-cameras".to_owned()]);
            assert!(r.licence_permits("public-cameras"), "{mode:?}");
        }
    }

    #[test]
    fn missing_keys_make_a_layer_unconfigured() {
        let mut usgs = sources::all().remove(0);
        usgs.required_secrets = vec![crate::secrets::SecretKey::FirmsMapKey];
        let r = Registry::with_sources(vec![usgs], LicenceMode::Evaluation, &[]);
        let none = MemorySecretStore::default();
        assert_eq!(
            r.availability(FeedId::Earthquakes, &none, true),
            Availability::Unconfigured { missing: vec!["NASA FIRMS map key".to_owned()] }
        );
        let with = MemorySecretStore::with([(crate::secrets::SecretKey::FirmsMapKey, "k")]);
        assert_eq!(r.availability(FeedId::Earthquakes, &with, true), Availability::Available);
    }

    #[test]
    fn vessels_need_a_key_and_unknown_terms_stay_off_in_commercial_mode() {
        let none = MemorySecretStore::default();
        let r = Registry::new(LicenceMode::Evaluation, &[]);
        assert_eq!(
            r.availability(FeedId::Vessels, &none, true),
            Availability::Unconfigured { missing: vec!["AISStream API key".to_owned()] }
        );
        let key = MemorySecretStore::with([(crate::secrets::SecretKey::AisStreamApiKey, "k")]);
        assert_eq!(r.availability(FeedId::Vessels, &key, true), Availability::Available);
        let commercial = Registry::new(LicenceMode::Commercial, &[]);
        assert_eq!(
            commercial.availability(FeedId::Vessels, &key, true),
            Availability::DisabledByLicence
        );
        let info = commercial.source(sources::AISSTREAM).unwrap().info.clone();
        assert_eq!(info.commercial, CommercialUse::Unknown);
        assert!(info.requires_key && info.notes.unwrap().contains("ENG-938"));
    }

    #[test]
    fn disabled_provider_is_unconfigured() {
        let r = Registry::new(LicenceMode::Evaluation, &[]);
        let secrets = MemorySecretStore::default();
        assert!(matches!(
            r.availability(FeedId::Aircraft, &secrets, false),
            Availability::Unconfigured { .. }
        ));
    }

    #[test]
    fn every_layer_source_is_registered() {
        let r = Registry::new(LicenceMode::Evaluation, &[]);
        for l in layers() {
            for s in &l.sources {
                assert!(r.source(s).is_some(), "{} → {s}", l.name);
            }
        }
    }
}
