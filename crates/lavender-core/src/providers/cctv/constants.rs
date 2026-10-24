//! Constants for the CCTV public-camera provider.

use lavender_domain::GeoPoint;
use std::time::Duration;

/// Default catalog-wide safety ceiling on served cameras.
pub const DEFAULT_CCTV_MAX_SOURCES: usize = 4000;

/// Hard upper bound for `CCTV_MAX_SOURCES`; also sizes the health map.
pub const CCTV_MAX_SOURCES_CEILING: usize = 5000;

/// Maximum number of concurrent live streams allowed.
pub const MAX_CONCURRENT_LIVE_STREAMS: usize = 8;

/// Individual CCTV image fetches must settle before the active client refresh cadence (8 seconds).
pub const CCTV_FRAME_FETCH_TIMEOUT: Duration = Duration::from_secs(8);

/// Maximum buffered snapshot size (16 MiB).
pub const CCTV_FRAME_MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

/// Deadline for upstream response headers (15 seconds).
pub const CCTV_MEDIA_FETCH_TIMEOUT: Duration = Duration::from_secs(15);

/// Silence a live body may carry before the relay gives up on it (30 seconds).
pub const CCTV_MEDIA_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Declared size ceiling for fixed media responses (64 MiB).
pub const CCTV_MEDIA_MAX_BODY_BYTES: usize = 64 * 1024 * 1024;

/// Maximum number of same-origin redirects to follow.
pub const MAX_SAME_HOST_REDIRECTS: usize = 2;

/// Longest unselected camera label the HUD shows before it gets noisy.
pub const CAMERA_CODE_MAX_CHARS: usize = 28;

// Pinned origins for frame fetches (SSRF defense)
pub const TXDOT_ORIGIN: &str = "https://its.txdot.gov";
pub const TXDOT_SNAPSHOT_PATH: &str = "/its/DistrictIts/GetCctvSnapshotByIcdId";
pub const TFL_IMAGE_ORIGIN: &str = "https://s3-eu-west-1.amazonaws.com/jamcams.tfl.gov.uk/";
pub const ONTARIO_511_IMAGE_ORIGIN: &str = "https://511on.ca/map/Cctv/";
pub const FINTRAFFIC_IMAGE_ORIGIN: &str = "https://weathercam.digitraffic.fi/";
pub const TALLINN_IMAGE_ORIGIN: &str = "https://ristmikud.tallinn.ee/";
pub const TARKTEE_IMAGE_ORIGIN: &str = "https://tarktee.transpordiamet.ee/images/";
pub const NSW_IMAGE_ORIGIN: &str = "https://webcams.transport.nsw.gov.au/";
pub const CALGARY_IMAGE_ORIGIN: &str = "https://trafficcam.calgary.ca/";
pub const VEGVESEN_IMAGE_ORIGIN: &str = "https://kamera.atlas.vegvesen.no/api/images/";
pub const WARENDORF_IMAGE_ORIGINS: &[&str] =
    &["http://webcam.warendorf.de/", "https://www.kreis-warendorf.de/"];

pub const NSW_IMAGE_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
pub const DEFAULT_PROXY_USER_AGENT: &str = "gods-eye-view-cctv-proxy/1.0";

// Anchors
pub const AUSTIN_DOWNTOWN: GeoPoint = GeoPoint { lat: 30.2672, lon: -97.7431, alt_m: None };
pub const LONDON_CENTER: GeoPoint = GeoPoint { lat: 51.5074, lon: -0.1278, alt_m: None };
pub const TALLINN_CENTER: GeoPoint = GeoPoint { lat: 59.437, lon: 24.753, alt_m: None };
pub const SYDNEY_CENTER: GeoPoint = GeoPoint { lat: -33.8688, lon: 151.2093, alt_m: None };
pub const CALGARY_DOWNTOWN: GeoPoint = GeoPoint { lat: 51.0461, lon: -114.0626, alt_m: None };

pub const TXDOT_ANCHORS: &[GeoPoint] = &[
    GeoPoint { lat: 30.2672, lon: -97.7431, alt_m: None }, // Austin
    GeoPoint { lat: 29.4241, lon: -98.4936, alt_m: None }, // San Antonio
    GeoPoint { lat: 29.7604, lon: -95.3698, alt_m: None }, // Houston
    GeoPoint { lat: 32.7767, lon: -96.7970, alt_m: None }, // Dallas
    GeoPoint { lat: 32.7555, lon: -97.3308, alt_m: None }, // Fort Worth
];

pub const CALTRANS_ANCHORS: &[GeoPoint] = &[
    GeoPoint { lat: 37.7793, lon: -122.4193, alt_m: None }, // San Francisco
    GeoPoint { lat: 34.0537, lon: -118.2428, alt_m: None }, // Los Angeles
    GeoPoint { lat: 32.7157, lon: -117.1611, alt_m: None }, // San Diego
    GeoPoint { lat: 38.5816, lon: -121.4944, alt_m: None }, // Sacramento
];

pub const ONTARIO_ANCHORS: &[GeoPoint] = &[
    GeoPoint { lat: 43.4516, lon: -80.4925, alt_m: None }, // Kitchener
    GeoPoint { lat: 43.6532, lon: -79.3832, alt_m: None }, // Toronto
    GeoPoint { lat: 45.4215, lon: -75.6972, alt_m: None }, // Ottawa
    GeoPoint { lat: 43.2557, lon: -79.8711, alt_m: None }, // Hamilton
    GeoPoint { lat: 42.9849, lon: -81.2453, alt_m: None }, // London, ON
    GeoPoint { lat: 42.3149, lon: -83.0364, alt_m: None }, // Windsor
];

pub const FINLAND_ANCHORS: &[GeoPoint] = &[
    GeoPoint { lat: 60.1699, lon: 24.9384, alt_m: None }, // Helsinki
    GeoPoint { lat: 60.4518, lon: 22.2666, alt_m: None }, // Turku
    GeoPoint { lat: 61.4978, lon: 23.7610, alt_m: None }, // Tampere
    GeoPoint { lat: 62.2426, lon: 25.7473, alt_m: None }, // Jyväskylä
    GeoPoint { lat: 62.8924, lon: 27.6770, alt_m: None }, // Kuopio
    GeoPoint { lat: 65.0121, lon: 25.4651, alt_m: None }, // Oulu
    GeoPoint { lat: 66.5039, lon: 25.7294, alt_m: None }, // Rovaniemi
];

pub const DRIVEBC_ANCHORS: &[GeoPoint] = &[
    GeoPoint { lat: 49.2827, lon: -123.1207, alt_m: None }, // Vancouver
    GeoPoint { lat: 48.4284, lon: -123.3656, alt_m: None }, // Victoria
];

pub const NORWAY_ANCHORS: &[GeoPoint] = &[
    GeoPoint { lat: 59.9139, lon: 10.7522, alt_m: None }, // Oslo
    GeoPoint { lat: 60.3913, lon: 5.3221, alt_m: None },  // Bergen
    GeoPoint { lat: 58.9700, lon: 5.7331, alt_m: None },  // Stavanger
    GeoPoint { lat: 63.4305, lon: 10.3951, alt_m: None }, // Trondheim
    GeoPoint { lat: 58.1467, lon: 7.9956, alt_m: None },  // Kristiansand
    GeoPoint { lat: 67.2804, lon: 14.4049, alt_m: None }, // Bodø
    GeoPoint { lat: 69.6492, lon: 18.9553, alt_m: None }, // Tromsø
];

pub const DELDOT_ANCHORS: &[GeoPoint] = &[
    GeoPoint { lat: 39.7459, lon: -75.5466, alt_m: None }, // Wilmington
    GeoPoint { lat: 39.1582, lon: -75.5244, alt_m: None }, // Dover
    GeoPoint { lat: 38.6906, lon: -75.3877, alt_m: None }, // Georgetown
];

pub const TARKTEE_ANCHORS: &[GeoPoint] = &[
    GeoPoint { lat: 59.4370, lon: 24.7530, alt_m: None }, // Tallinn
    GeoPoint { lat: 58.3780, lon: 26.7290, alt_m: None }, // Tartu
    GeoPoint { lat: 58.3859, lon: 24.4971, alt_m: None }, // Pärnu
    GeoPoint { lat: 59.3797, lon: 28.1791, alt_m: None }, // Narva
];
