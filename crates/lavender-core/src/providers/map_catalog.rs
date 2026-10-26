//! Base map imagery catalog and credential-aware availability (`LB-119`), porting
//! `legacy/src/maps/catalog.js` and `legacy/src/maps/availability.js`.

use lavender_domain::{MapCatalogSnapshot, MapStackEntry, MapStackKind};

/// Credentials the operator has configured; only presence matters.
#[derive(Debug, Clone, Copy, Default)]
pub struct MapCredentials {
    pub google_maps: bool,
    pub cesium_ion: bool,
}

/// Why Google 3D is unavailable, recommending the right fix (`photorealUnavailableReason`).
#[must_use]
pub fn photoreal_unavailable_reason(has_credentials: bool) -> String {
    if has_credentials {
        "Google 3D tiles unavailable — check the key's API restrictions, quota, or network"
            .to_owned()
    } else {
        "Needs LAVENDER_GOOGLE_MAPS_API_KEY — add it in Provider Settings — or a Cesium ion token for the ion-hosted route"
            .to_owned()
    }
}

fn entry(
    id: &str,
    label: &str,
    short_label: &str,
    kind: MapStackKind,
    requires_ion: bool,
    credit_html: &str,
) -> MapStackEntry {
    MapStackEntry {
        id: id.to_owned(),
        label: label.to_owned(),
        short_label: short_label.to_owned(),
        kind,
        requires_ion,
        available: true,
        unavailable_reason: None,
        credit_html: credit_html.to_owned(),
    }
}

/// Builds the five canonical stacks with availability resolved from `credentials`.
#[must_use]
pub fn map_catalog(credentials: MapCredentials) -> MapCatalogSnapshot {
    let mut stacks = vec![
        entry("photoreal", "Google 3D", "3D", MapStackKind::Photoreal, false, "Google"),
        entry("bing-aerial", "Bing Aerial", "Aerial", MapStackKind::Ion, true, "Microsoft Bing"),
        entry("bing-labels", "Bing Labels", "Labels", MapStackKind::Ion, true, "Microsoft Bing"),
        entry(
            "esri-imagery",
            "Esri Satellite",
            "SAT",
            MapStackKind::EsriImagery,
            false,
            "Esri, Maxar, Earthstar Geographics",
        ),
        entry("osm", "OSM", "OSM", MapStackKind::Osm, false, "© OpenStreetMap contributors"),
    ];
    for stack in &mut stacks {
        if stack.kind == MapStackKind::Photoreal
            && !(credentials.google_maps || credentials.cesium_ion)
        {
            stack.available = false;
            stack.unavailable_reason = Some(photoreal_unavailable_reason(false));
        } else if stack.requires_ion && !credentials.cesium_ion {
            stack.available = false;
            stack.unavailable_reason = Some("Needs a Cesium ion token".to_owned());
        }
    }
    MapCatalogSnapshot { stacks }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_credentials_only_keyless_stacks_are_available() {
        let snap = map_catalog(MapCredentials::default());
        let available: Vec<&str> =
            snap.stacks.iter().filter(|s| s.available).map(|s| s.id.as_str()).collect();
        assert_eq!(available, ["esri-imagery", "osm"]);
        let photoreal = snap.stacks.iter().find(|s| s.id == "photoreal");
        assert!(
            photoreal
                .and_then(|s| s.unavailable_reason.as_deref())
                .is_some_and(|r| r.contains("Provider Settings"))
        );
    }

    #[test]
    fn ion_token_unlocks_bing_and_photoreal() {
        let snap = map_catalog(MapCredentials { google_maps: false, cesium_ion: true });
        assert!(snap.stacks.iter().all(|s| s.available));
    }

    #[test]
    fn configured_key_failure_does_not_ask_for_a_key_again() {
        assert!(!photoreal_unavailable_reason(true).contains("Needs"));
    }
}
