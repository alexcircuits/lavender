//! Registry of approved, openly licensed GTFS-Realtime feeds.

use lavender_domain::{GeoPoint, TransitFeed};

/// Returns whether `id` is a structurally valid transit feed identifier.
#[must_use]
pub fn valid_transit_feed_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 64 {
        return false;
    }
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn make_point(lat: f64, lon: f64) -> GeoPoint {
    match GeoPoint::new(lat, lon) {
        Ok(point) => point,
        Err(_) => GeoPoint { lat, lon, alt_m: None },
    }
}

/// Detects MBTA transit mode from route identifier.
#[must_use]
pub fn mbta_route_mode(route_id: &str) -> &'static str {
    if route_id.starts_with("Red") || route_id.starts_with("Orange") || route_id.starts_with("Blue")
    {
        "subway"
    } else if route_id.starts_with("Green") || route_id.starts_with("Mattapan") {
        "tram"
    } else if route_id.starts_with("CR-") {
        "rail"
    } else if route_id.starts_with("Boat-") {
        "ferry"
    } else {
        "bus"
    }
}

/// Detects HSL (Helsinki) transit mode from route identifier.
#[must_use]
pub fn hsl_route_mode(route_id: &str) -> &'static str {
    if route_id.starts_with("31M") {
        "subway"
    } else if route_id.starts_with("1019") {
        "ferry"
    } else if route_id.starts_with("10") && route_id.len() >= 4 {
        "tram"
    } else if route_id.starts_with("300") {
        "rail"
    } else {
        "bus"
    }
}

/// Detects Metro Transit (Minneapolis-St Paul) mode from route identifier.
#[must_use]
pub fn metro_transit_route_mode(route_id: &str) -> &'static str {
    if route_id == "901" || route_id == "902" {
        "tram"
    } else if route_id == "888" {
        "rail"
    } else {
        "bus"
    }
}

/// Detects Entur (Norway) transit mode from route identifier.
#[must_use]
pub fn entur_route_mode(route_id: &str) -> &'static str {
    let codespace = route_id.split(':').next().unwrap_or("");
    match codespace {
        "VYG" | "GJB" | "SJN" | "FLT" | "GOA" | "NSB" | "VYT" | "FLB" => "rail",
        _ => "bus",
    }
}

/// Resolves the effective transit mode given a feed and optional route identifier.
#[must_use]
pub fn resolve_route_mode(feed_id: &str, route_id: Option<&str>, default_mode: &str) -> String {
    let Some(route) = route_id else {
        return default_mode.to_string();
    };
    match feed_id {
        "mbta" => mbta_route_mode(route).to_string(),
        "hsl-helsinki" => hsl_route_mode(route).to_string(),
        "metrotransit-msp" => metro_transit_route_mode(route).to_string(),
        "entur-norway" => entur_route_mode(route).to_string(),
        _ => default_mode.to_string(),
    }
}

/// Returns the complete registry of default approved GTFS-Realtime feeds.
#[must_use]
pub fn default_approved_feeds() -> Vec<TransitFeed> {
    vec![
        TransitFeed {
            id: "mbta".to_string(),
            name: "MBTA".to_string(),
            operator: "Massachusetts Bay Transportation Authority".to_string(),
            region: "Boston, MA".to_string(),
            center: make_point(42.3601, -71.0589),
            load_radius_km: 70.0,
            url: "https://cdn.mbta.com/realtime/VehiclePositions.pb".to_string(),
            license: "MassDOT Developers License Agreement".to_string(),
            attribution: "MBTA / MassDOT".to_string(),
            default_enabled: true,
            default_mode: "bus".to_string(),
        },
        TransitFeed {
            id: "capmetro-austin".to_string(),
            name: "CapMetro".to_string(),
            operator: "Capital Metropolitan Transportation Authority".to_string(),
            region: "Austin, TX".to_string(),
            center: make_point(30.2672, -97.7431),
            load_radius_km: 60.0,
            url: "https://data.texas.gov/download/eiei-9rpf/application%2Foctet-stream".to_string(),
            license: "CapMetro Developer Tools license".to_string(),
            attribution: "Capital Metropolitan Transportation Authority — data.texas.gov"
                .to_string(),
            default_enabled: true,
            default_mode: "bus".to_string(),
        },
        TransitFeed {
            id: "metrotransit-msp".to_string(),
            name: "Metro Transit".to_string(),
            operator: "Metro Transit (Metropolitan Council)".to_string(),
            region: "Minneapolis–St Paul, MN".to_string(),
            center: make_point(44.9778, -93.2650),
            load_radius_km: 70.0,
            url: "https://svc.metrotransit.org/mtgtfs/vehiclepositions.pb".to_string(),
            license: "Public Metro Transit vehicle position data".to_string(),
            attribution: "Metro Transit — Metropolitan Council".to_string(),
            default_enabled: true,
            default_mode: "bus".to_string(),
        },
        TransitFeed {
            id: "hsl-helsinki".to_string(),
            name: "HSL".to_string(),
            operator: "Helsinki Region Transport (HSL)".to_string(),
            region: "Helsinki, Finland".to_string(),
            center: make_point(60.1699, 24.9384),
            load_radius_km: 70.0,
            url: "https://realtime.hsl.fi/realtime/vehicle-positions/v2/hsl".to_string(),
            license: "CC BY 4.0".to_string(),
            attribution: "HSL (Helsinki Region Transport)".to_string(),
            default_enabled: true,
            default_mode: "bus".to_string(),
        },
        TransitFeed {
            id: "ovapi-nl".to_string(),
            name: "OVapi".to_string(),
            operator: "Stichting OpenGeo (NDOV data)".to_string(),
            region: "Netherlands".to_string(),
            center: make_point(52.2000, 5.3000),
            load_radius_km: 220.0,
            url: "https://gtfs.ovapi.nl/nl/vehiclePositions.pb".to_string(),
            license: "Free to use per the OVapi README (best effort, no SLA)".to_string(),
            attribution: "OVapi / Stichting OpenGeo — Dutch integrated real-time transit data"
                .to_string(),
            default_enabled: true,
            default_mode: "bus".to_string(),
        },
        TransitFeed {
            id: "entur-norway".to_string(),
            name: "Entur".to_string(),
            operator: "Entur AS (Norwegian national transit data)".to_string(),
            region: "Norway".to_string(),
            center: make_point(64.0000, 11.5000),
            load_radius_km: 720.0,
            url: "https://api.entur.io/realtime/v1/gtfs-rt/vehicle-positions".to_string(),
            license: "Norwegian Licence for Open Government Data (NLOD)".to_string(),
            attribution: "Entur — data under NLOD".to_string(),
            default_enabled: true,
            default_mode: "bus".to_string(),
        },
        TransitFeed {
            id: "translink-seq".to_string(),
            name: "TransLink".to_string(),
            operator: "TransLink (Queensland Government)".to_string(),
            region: "South East Queensland, Australia".to_string(),
            center: make_point(-27.4698, 153.0251),
            load_radius_km: 150.0,
            url: "https://gtfsrt.api.translink.com.au/api/realtime/seq/VehiclePositions"
                .to_string(),
            license: "CC BY 4.0".to_string(),
            attribution: "TransLink — Queensland Government (CC BY 4.0)".to_string(),
            default_enabled: true,
            default_mode: "bus".to_string(),
        },
    ]
}
