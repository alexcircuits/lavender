//! The source registry's facts: who provides each datum, under which licence, with which
//! mandated attribution (docs/audit/data-sources-and-licensing.md). This is the executable
//! form of the licensing matrix (ADR 0008).

use lavender_domain::{
    CommercialUse, ImageryDescriptor, ImageryKind, Licence, Privacy, SourceInfo,
};

use crate::secrets::SecretKey;

pub const USGS: &str = "usgs-earthquakes";
pub const ADSB_LOL: &str = "adsb-lol";
pub const CELESTRAK: &str = "celestrak";
pub const NASA_GIBS: &str = "nasa-gibs";
pub const NATURAL_EARTH: &str = "natural-earth";

/// A source plus what it needs to run.
#[derive(Debug, Clone)]
pub struct SourceDef {
    pub info: SourceInfo,
    pub required_secrets: Vec<SecretKey>,
}

fn source(
    id: &str,
    name: &str,
    homepage: &str,
    attribution: &str,
    licence: (&str, &str),
    commercial: CommercialUse,
    notes: Option<&str>,
) -> SourceDef {
    SourceDef {
        info: SourceInfo {
            id: id.to_owned(),
            name: name.to_owned(),
            homepage: homepage.to_owned(),
            attribution: attribution.to_owned(),
            licence: Licence { name: licence.0.to_owned(), url: licence.1.to_owned() },
            commercial,
            requires_key: false,
            privacy: Privacy::Normal,
            notes: notes.map(str::to_owned),
        },
        required_secrets: Vec::new(),
    }
}

/// Every source Lavender can use, in display order.
#[must_use]
pub fn all() -> Vec<SourceDef> {
    vec![
        source(
            USGS,
            "USGS Earthquake Hazards Program",
            "https://earthquake.usgs.gov/earthquakes/feed/",
            "Data: U.S. Geological Survey",
            (
                "Public domain (U.S. Government work)",
                "https://www.usgs.gov/information-policies-and-instructions/copyrights-and-credits",
            ),
            CommercialUse::Allowed,
            Some("No endorsement implied; USGS logos may not be used."),
        ),
        source(
            ADSB_LOL,
            "adsb.lol",
            "https://adsb.lol/",
            "Aircraft data: adsb.lol (ODbL 1.0)",
            ("Open Database License 1.0", "https://opendatacommons.org/licenses/odbl/1-0/"),
            CommercialUse::Allowed,
            Some("Share-alike applies to derived databases that are redistributed."),
        ),
        source(
            CELESTRAK,
            "CelesTrak",
            "https://celestrak.org/",
            "Orbital elements: CelesTrak (celestrak.org), Dr. T.S. Kelso",
            ("No licence published", "https://celestrak.org/"),
            CommercialUse::Unknown,
            Some(
                "CelesTrak grants no explicit licence; commercial basis pending an owner decision (ENG-938).",
            ),
        ),
        source(
            NASA_GIBS,
            "NASA Global Imagery Browse Services",
            "https://www.earthdata.nasa.gov/engage/open-data-services-software/earthdata-developer-portal/gibs-api",
            "We acknowledge the use of imagery provided by services from NASA's Global Imagery Browse Services (GIBS), part of NASA's Earth Science Data and Information System (ESDIS).",
            (
                "NASA open data policy",
                "https://www.earthdata.nasa.gov/engage/open-data-services-software-policies",
            ),
            CommercialUse::Allowed,
            None,
        ),
        source(
            NATURAL_EARTH,
            "Natural Earth (bundled with CesiumJS)",
            "https://www.naturalearthdata.com/",
            "Made with Natural Earth",
            ("Public domain", "https://www.naturalearthdata.com/about/terms-of-use/"),
            CommercialUse::Allowed,
            Some("Bundled offline imagery; works without network access."),
        ),
    ]
}

const GIBS_WMTS: &str = "https://gibs.earthdata.nasa.gov/wmts/epsg4326/best/";

/// Base imagery the globe can show. Identifiers and tile matrix sets were confirmed against
/// GIBS `GetCapabilities` (2026-10-11).
#[must_use]
pub fn imagery() -> Vec<ImageryDescriptor> {
    let gibs = |id: &str,
                name: &str,
                layer: &str,
                ext: &str,
                time: Option<&str>,
                dark: bool,
                default: bool| {
        let time_segment = time.map(|t| format!("{t}/")).unwrap_or_default();
        ImageryDescriptor {
            id: id.to_owned(),
            name: name.to_owned(),
            kind: ImageryKind::WmtsGeographic,
            url: format!(
                "{GIBS_WMTS}{layer}/default/{time_segment}500m/{{TileMatrix}}/{{TileRow}}/{{TileCol}}.{ext}"
            ),
            layer: Some(layer.to_owned()),
            tile_matrix_set: Some("500m".to_owned()),
            format: Some(if ext == "png" { "image/png" } else { "image/jpeg" }.to_owned()),
            maximum_level: 7,
            source_id: NASA_GIBS.to_owned(),
            tile_hosts: vec!["gibs.earthdata.nasa.gov".to_owned()],
            is_dark: dark,
            is_default: default,
        }
    };
    vec![
        gibs(
            "gibs-black-marble",
            "Night lights (VIIRS Black Marble 2016)",
            "VIIRS_Black_Marble",
            "png",
            Some("2016-01-01"),
            true,
            true,
        ),
        gibs(
            "gibs-blue-marble",
            "Relief and bathymetry (Blue Marble)",
            "BlueMarble_ShadedRelief_Bathymetry",
            "jpeg",
            None,
            false,
            false,
        ),
        ImageryDescriptor {
            id: "natural-earth-ii".to_owned(),
            name: "Natural Earth II (offline)".to_owned(),
            kind: ImageryKind::Bundled,
            url: String::new(),
            layer: None,
            tile_matrix_set: None,
            format: None,
            maximum_level: 2,
            source_id: NATURAL_EARTH.to_owned(),
            tile_hosts: Vec::new(),
            is_dark: false,
            is_default: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_source_has_complete_licence_facts() {
        for s in all() {
            let i = &s.info;
            assert!(!i.attribution.trim().is_empty(), "{} lacks attribution", i.id);
            assert!(!i.licence.name.trim().is_empty(), "{} lacks licence name", i.id);
            assert!(i.licence.url.starts_with("https://"), "{} licence url", i.id);
            assert!(i.homepage.starts_with("https://"), "{} homepage", i.id);
        }
    }

    #[test]
    fn every_imagery_layer_names_a_registered_source_and_one_default() {
        let ids: Vec<String> = all().into_iter().map(|s| s.info.id).collect();
        let imagery = imagery();
        assert_eq!(imagery.iter().filter(|i| i.is_default).count(), 1);
        for i in &imagery {
            assert!(ids.contains(&i.source_id), "{} → {}", i.id, i.source_id);
            if i.kind == ImageryKind::WmtsGeographic {
                assert!(i.url.contains("{TileMatrix}") && i.url.starts_with("https://"));
            }
        }
    }
}
