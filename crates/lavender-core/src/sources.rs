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
pub const NOAA_CYCLONES: &str = "noaa-nhc-cphc";
pub const NOAA_GFS: &str = "noaa-gfs";
pub const ECMWF_IFS: &str = "ecmwf-ifs";
pub const LAUNCH_LIBRARY_2: &str = "launch-library-2";
pub const NASA_FIRMS: &str = "nasa-firms";
pub const NASA_GIBS: &str = "nasa-gibs";
pub const NATURAL_EARTH: &str = "natural-earth";
pub const AISSTREAM: &str = "aisstream";

/// Credit for GFS wind. NOAA data are public domain; the NWS disclaimer forbids implying
/// endorsement or presenting modified data as official.
pub const NOAA_GFS_ATTRIBUTION: &str = "Wind: NOAA Global Forecast System (GFS) via NOAA Open Data Dissemination (U.S. public domain), resampled by Lavender; not an official NOAA/NWS product and no endorsement implied.";
/// The statement ECMWF's terms require of services built with its open data (copyright,
/// source, CC BY 4.0 licence, modification and disclaimer).
pub const ECMWF_IFS_ATTRIBUTION: &str = "This service is based on data and products of the European Centre for Medium-Range Weather Forecasts (ECMWF). Source: www.ecmwf.int. This ECMWF data is published under a Creative Commons Attribution 4.0 International (CC BY 4.0) licence: https://creativecommons.org/licenses/by/4.0/. Modified: 10 m wind (and optional 2 m temperature and mean sea level pressure) decoded and resampled to a regular grid for display. ECMWF does not accept any liability whatsoever for any error or omission in the data, their availability, or for any loss or damage arising from their use.";

pub const NIFC_WFIGS: &str = "nifc-wfigs";
pub const INCIWEB: &str = "inciweb";
pub const MAPILLARY: &str = "mapillary";

pub const CCTV_AUSTIN: &str = "cctv-austin";
pub const CCTV_CALTRANS: &str = "cctv-caltrans";
pub const CCTV_TFL: &str = "cctv-tfl";
pub const CCTV_ONTARIO: &str = "cctv-ontario";
pub const CCTV_FINTRAFFIC: &str = "cctv-fintraffic";
pub const CCTV_DRIVEBC: &str = "cctv-drivebc";
pub const CCTV_TXDOT: &str = "cctv-txdot";
pub const CCTV_TALLINN: &str = "cctv-tallinn";
pub const CCTV_TARKTEE: &str = "cctv-tarktee";
pub const CCTV_WARENDORF: &str = "cctv-warendorf";
pub const CCTV_NSW: &str = "cctv-nsw";
pub const CCTV_CALGARY: &str = "cctv-calgary";
pub const CCTV_DELDOT: &str = "cctv-deldot";
pub const CCTV_VEGVESEN: &str = "cctv-vegvesen";
pub const OSRM: &str = "osrm-fossgis";
pub const GBFS: &str = "gbfs";
pub const GTFS_RT: &str = "gtfs-rt";
pub const REEARTH_TERRAIN: &str = "reearth-terrain";
pub const TOMTOM_TRAFFIC: &str = "tomtom-traffic";
pub const OPENFREEMAP: &str = "openfreemap";
pub const OPENSKY: &str = "opensky-network";
pub const ADS_B_DB: &str = "adsbdb";
pub const TELEGEOGRAPHY: &str = "telegeography-submarine-cables";
pub const NOAA_NOWCOAST: &str = "noaa-nowcoast";
pub const OSM_INFRASTRUCTURE: &str = "osm-infrastructure";
pub const OSM_MILITARY: &str = "osm-military";
pub const NASA_CMR: &str = "nasa-cmr";
pub const NATURAL_EARTH_DIVISIONS: &str = "natural-earth-divisions";
pub const GNSS_INTEGRITY: &str = "gnss-integrity";
pub const ECCC_AQHI: &str = "eccc-aqhi";
pub const AIR_QUALITY: &str = "air-quality";

/// Attribution for Environment and Climate Change Canada AQHI observations.
pub const ECCC_AQHI_ATTRIBUTION: &str = "Contains information licensed under the Environment and Climate Change Canada Data Servers End-use Licence";

/// Attribution for GNSS navigation integrity analysis.
pub const GNSS_INTEGRITY_ATTRIBUTION: &str = "GNSS integrity analysis: derived from aircraft ADS-B navigation accuracy indicators (ODbL 1.0); anomaly classification based on 14 CFR 91.227 and gpsjam.org methodology.";

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
    source_with_secrets(id, name, homepage, attribution, licence, commercial, notes, Vec::new())
}

fn sensitive_source(
    id: &str,
    name: &str,
    homepage: &str,
    attribution: &str,
    licence: (&str, &str),
    commercial: CommercialUse,
    notes: Option<&str>,
) -> SourceDef {
    let mut s = source(id, name, homepage, attribution, licence, commercial, notes);
    s.info.privacy = Privacy::Sensitive;
    s
}

#[allow(clippy::too_many_arguments)]
fn source_with_secrets(
    id: &str,
    name: &str,
    homepage: &str,
    attribution: &str,
    licence: (&str, &str),
    commercial: CommercialUse,
    notes: Option<&str>,
    required_secrets: Vec<SecretKey>,
) -> SourceDef {
    let requires_key = !required_secrets.is_empty();
    SourceDef {
        info: SourceInfo {
            id: id.to_owned(),
            name: name.to_owned(),
            homepage: homepage.to_owned(),
            attribution: attribution.to_owned(),
            licence: Licence { name: licence.0.to_owned(), url: licence.1.to_owned() },
            commercial,
            requires_key,
            privacy: Privacy::Normal,
            notes: notes.map(str::to_owned),
        },
        required_secrets,
    }
}

/// Every source Lavender can use, in display order.
#[must_use]
#[allow(clippy::too_many_lines)]
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
            NOAA_CYCLONES,
            "NOAA National Hurricane Center / Central Pacific Hurricane Center",
            "https://www.nhc.noaa.gov/",
            "NOAA/NWS National Hurricane Center / Central Pacific Hurricane Center; no endorsement implied",
            ("Public domain (NOAA/NWS data)", "https://www.weather.gov/disclaimer"),
            CommercialUse::Allowed,
            Some(
                "NWS data only; no Esri basemap imagery. Lavender projects these data for display and is not an official NOAA/NWS product. Atlantic and eastern/central North Pacific coverage. Cones show centre-track uncertainty, not the extent of hazards.",
            ),
        ),
        source(
            NOAA_GFS,
            "NOAA Global Forecast System (GFS)",
            "https://www.ncei.noaa.gov/products/weather-climate-models/global-forecast",
            NOAA_GFS_ATTRIBUTION,
            ("Public domain (NOAA data)", "https://www.weather.gov/disclaimer"),
            CommercialUse::Allowed,
            Some(
                "NOAA Open Data Dissemination on AWS (noaa-gfs-bdp-pds); 0.25° forecast, not observations. Lavender resamples the data for display; not an official NOAA/NWS product. NOAA/NWS names and logos need permission.",
            ),
        ),
        source(
            ECMWF_IFS,
            "ECMWF Integrated Forecasting System (IFS)",
            "https://www.ecmwf.int/en/forecasts/datasets/open-data",
            ECMWF_IFS_ATTRIBUTION,
            (
                "Creative Commons Attribution 4.0 International (CC BY 4.0)",
                "https://creativecommons.org/licenses/by/4.0/",
            ),
            CommercialUse::Allowed,
            Some(
                "ECMWF open data real-time forecast (data.ecmwf.int, limited to 500 simultaneous connections; also mirrored on AWS, Azure and Google Cloud). Attribution text follows the ECMWF terms for services built with ECMWF data (https://apps.ecmwf.int/datasets/licences/general/, checked 2026-10-15).",
            ),
        ),
        source(
            LAUNCH_LIBRARY_2,
            "The Space Devs — Launch Library 2",
            "https://thespacedevs.com/llapi",
            "Launch Library 2 — The Space Devs",
            (
                "The Space Devs terms of use",
                "https://github.com/TheSpaceDevs/Tutorials/blob/main/faqs/faq_TSD.md#terms-of-use",
            ),
            CommercialUse::Allowed,
            Some("15 requests per hour free tier; cache responses server-side."),
        ),
        source_with_secrets(
            NASA_FIRMS,
            "NASA FIRMS (Fire Information for Resource Management System)",
            "https://firms.modaps.eosdis.nasa.gov/",
            "NASA FIRMS; MODIS and VIIRS active fire data",
            (
                "NASA open data policy",
                "https://www.earthdata.nasa.gov/engage/open-data-services-software-policies",
            ),
            CommercialUse::Allowed,
            Some(
                "Requires a NASA FIRMS MAP_KEY. Active fire detections from MODIS and VIIRS instruments.",
            ),
            vec![SecretKey::FirmsMapKey],
        ),
        source_with_secrets(
            AISSTREAM,
            "AISStream",
            "https://aisstream.io/",
            "Vessel positions: AISStream.io",
            ("No terms of use published", "https://aisstream.io/documentation"),
            CommercialUse::Unknown,
            Some(
                "AISStream publishes no terms of use or data licence (licensing audit; commercial basis pending an owner decision, ENG-938). Off unless an API key is configured. Public AIS broadcasts relayed by terrestrial receivers: coverage is coastal and sparse at sea. Positions and short tracks are kept in memory only, for at most the configured retention.",
            ),
            vec![SecretKey::AisStreamApiKey],
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
        source(
            NIFC_WFIGS,
            "NIFC Wildland Fire Interagency Geospatial Services (WFIGS)",
            "https://data-nifc.opendata.arcgis.com/",
            "Wildfire perimeters: National Interagency Fire Center (WFIGS)",
            (
                "U.S. interagency public data; NIFC terms of use (no-warranty disclaimer)",
                "https://www.arcgis.com/home/item.html?id=d1c32af3212341869b3c810f1a215824",
            ),
            CommercialUse::Allowed,
            Some(
                "Current US wildland fire perimeters, republished every 5 minutes; perimeters are not available for every incident. Item terms of use (retrieved 2026-10-15) are a disclaimer without use restrictions: the data are dynamic, not legal documents and carry no warranty. Contributors: DOI Office of Wildland Fire (IRWIN), USDA Forest Service, NPS, FWS, BIA, BLM, National Association of State Foresters, US Fire Administration and NWCG.",
            ),
        ),
        source(
            INCIWEB,
            "InciWeb (Incident Information System)",
            "https://inciweb.wildfire.gov/",
            "Incident information: InciWeb (inciweb.wildfire.gov)",
            ("U.S. Government public incident information", "https://inciweb.wildfire.gov/about"),
            CommercialUse::Allowed,
            Some(
                "Used only to link a perimeter to its incident page; page content stays on InciWeb. InciWeb states its information is for information purposes only and refers to the local wildfire agency for official information.",
            ),
        ),
        source_with_secrets(
            MAPILLARY,
            "Mapillary",
            "https://www.mapillary.com/",
            "Mapillary street-level imagery (CC BY-SA 4.0)",
            (
                "Creative Commons Attribution-ShareAlike 4.0 International",
                "https://creativecommons.org/licenses/by-sa/4.0/",
            ),
            CommercialUse::Allowed,
            Some("Vector coverage tiles and street-level imagery metadata."),
            vec![SecretKey::MapillaryClientToken],
        ),
        sensitive_source(
            CCTV_AUSTIN,
            "City of Austin Open Data (Traffic Cameras)",
            "https://data.austintexas.gov/",
            "City of Austin Open Data portal",
            ("Public domain / City of Austin Open Data", "https://data.austintexas.gov/"),
            CommercialUse::Allowed,
            Some("Public traffic cameras; privacy-sensitive CCTV layer."),
        ),
        sensitive_source(
            CCTV_CALTRANS,
            "Caltrans CCTV (California DOT)",
            "https://cwwp2.dot.ca.gov/",
            "California Department of Transportation (Caltrans)",
            ("Public domain / Caltrans highway camera feed", "https://dot.ca.gov/"),
            CommercialUse::Allowed,
            Some("Statewide highway traffic cameras; privacy-sensitive CCTV layer."),
        ),
        sensitive_source(
            CCTV_TFL,
            "Transport for London (JamCams)",
            "https://tfl.gov.uk/",
            "Powered by TfL Open Data",
            (
                "Powered by TfL Open Data (requires registration)",
                "https://tfl.gov.uk/info-for/open-data-users/terms-and-conditions",
            ),
            CommercialUse::RequiresAgreement,
            Some("London traffic cameras; requires registration for commercial use."),
        ),
        sensitive_source(
            CCTV_ONTARIO,
            "Ontario 511 (Ministry of Transportation Ontario)",
            "https://511on.ca/",
            "Ministry of Transportation Ontario (511on.ca)",
            (
                "Open Government Licence – Ontario",
                "https://www.ontario.ca/page/open-government-licence-ontario",
            ),
            CommercialUse::Allowed,
            Some("Provincial highway cameras; privacy-sensitive CCTV layer."),
        ),
        sensitive_source(
            CCTV_FINTRAFFIC,
            "Fintraffic Weather Cameras (Digitraffic)",
            "https://www.digitraffic.fi/",
            "Fintraffic / digitraffic.fi (CC BY 4.0)",
            (
                "Creative Commons Attribution 4.0 International",
                "https://creativecommons.org/licenses/by/4.0/",
            ),
            CommercialUse::Allowed,
            Some("Road weather camera presets across Finland."),
        ),
        sensitive_source(
            CCTV_DRIVEBC,
            "DriveBC Highway Webcams",
            "https://www.drivebc.ca/",
            "DriveBC, Open Government Licence – British Columbia",
            (
                "Open Government Licence – British Columbia",
                "https://www2.gov.bc.ca/gov/content/data/open-data/open-government-licence-bc",
            ),
            CommercialUse::Allowed,
            Some("British Columbia provincial highway cameras."),
        ),
        sensitive_source(
            CCTV_TXDOT,
            "TxDOT ITS Traffic Cameras",
            "https://its.txdot.gov/",
            "Texas Department of Transportation (TxDOT)",
            ("Public TxDOT traffic camera data", "https://www.txdot.gov/"),
            CommercialUse::Allowed,
            Some("Texas statewide traffic and ITS camera feeds."),
        ),
        sensitive_source(
            CCTV_TALLINN,
            "Tallinn Traffic Cameras (Ristmikud)",
            "https://ristmikud.tallinn.ee/",
            "City of Tallinn (ristmikud.tallinn.ee)",
            ("Public City of Tallinn traffic camera data", "https://www.tallinn.ee/"),
            CommercialUse::NonCommercialOnly,
            Some("Curated intersection cameras in Tallinn; non-commercial only."),
        ),
        sensitive_source(
            CCTV_TARKTEE,
            "Transpordiamet Tarktee Road Cameras",
            "https://tarktee.ee/",
            "Estonian Transport Administration (Transpordiamet)",
            ("Public Transpordiamet road weather camera data", "https://transpordiamet.ee/"),
            CommercialUse::Allowed,
            Some("DATEX2 road weather camera feeds in Estonia."),
        ),
        sensitive_source(
            CCTV_WARENDORF,
            "Kreis Warendorf Municipal Webcam",
            "https://www.kreis-warendorf.de/",
            "Stadt Warendorf / Kreis Warendorf",
            ("Public municipal webcam data", "https://www.warendorf.de/"),
            CommercialUse::NonCommercialOnly,
            Some("Curated Marktplatz municipal webcam; non-commercial only."),
        ),
        sensitive_source(
            CCTV_NSW,
            "Live Traffic NSW (Transport for NSW)",
            "https://www.livetraffic.com/",
            "Live Traffic NSW — Transport for NSW (CC BY 4.0)",
            (
                "Creative Commons Attribution 4.0 International",
                "https://creativecommons.org/licenses/by/4.0/",
            ),
            CommercialUse::Allowed,
            Some("Transport for NSW live camera network."),
        ),
        sensitive_source(
            CCTV_CALGARY,
            "City of Calgary Traffic Cameras",
            "https://data.calgary.ca/",
            "The City of Calgary",
            (
                "Open Government Licence – City of Calgary",
                "https://data.calgary.ca/stories/s/Open-Calgary-Terms-of-Use/u45n-7awa/",
            ),
            CommercialUse::Allowed,
            Some("Open Calgary traffic camera feeds."),
        ),
        sensitive_source(
            CCTV_DELDOT,
            "Delaware DOT Traffic Cameras",
            "https://tmc.deldot.gov/",
            "Delaware Department of Transportation (DelDOT)",
            ("Public DelDOT traffic camera data", "https://deldot.gov/"),
            CommercialUse::Allowed,
            Some("Delaware statewide traffic cameras."),
        ),
        sensitive_source(
            CCTV_VEGVESEN,
            "Statens vegvesen Road Cameras (Norway)",
            "https://www.vegvesen.no/",
            "Statens vegvesen (Norway)",
            (
                "Norwegian Licence for Open Government Data (NLOD)",
                "https://data.norge.no/nlod/en/2.0",
            ),
            CommercialUse::Allowed,
            Some("Norwegian national road cameras (DATEX 3.1)."),
        ),
        source(
            OSRM,
            "OSRM on FOSSGIS",
            "https://routing.openstreetmap.de/about.html",
            "Routing data © OpenStreetMap contributors (ODbL). Improve this map: https://www.openstreetmap.org/fixthemap",
            (
                "FOSSGIS server terms of use / ODbL",
                "https://www.fossgis.de/arbeitsgruppen/osm-server/nutzungsbedingungen/",
            ),
            CommercialUse::NonCommercialOnly,
            Some(
                "Paced routing over OpenStreetMap via FOSSGIS OSRM servers (1 req/s floor, bounded queue). High-traffic commercial use requires self-hosting or agreement.",
            ),
        ),
        source(
            GBFS,
            "General Bikeshare Feed Specification (GBFS)",
            "https://gbfs.org/",
            "Station and availability data provided by regional bikeshare operators under GBFS open data terms.",
            ("GBFS Open Data Terms", "https://gbfs.org/"),
            CommercialUse::Allowed,
            Some("Live and static bikeshare station feeds from approved municipal systems."),
        ),
        source(
            GTFS_RT,
            "General Transit Feed Specification - Realtime (GTFS-RT)",
            "https://gtfs.org/realtime/",
            "Live public transit vehicle positions provided by regional transit agencies under open data terms.",
            ("GTFS-RT Open Data Terms", "https://gtfs.org/realtime/"),
            CommercialUse::Allowed,
            Some(
                "Live public transit vehicle position feeds from approved transit agency endpoints.",
            ),
        ),
        source(
            REEARTH_TERRAIN,
            "Re:Earth Terrain / Mapterhorn",
            "https://terrain.reearth.land/",
            "Terrain: Re:Earth Terrain · Mapterhorn (CC BY 4.0)",
            (
                "Creative Commons Attribution 4.0 International",
                "https://creativecommons.org/licenses/by/4.0/",
            ),
            CommercialUse::Allowed,
            Some("Batched ellipsoidal ground height queries; CC BY 4.0 open elevation data."),
        ),
        source_with_secrets(
            TOMTOM_TRAFFIC,
            "TomTom Traffic Flow",
            "https://developer.tomtom.com/traffic-api/documentation/traffic-flow/raster-flow-tiles",
            "Traffic flow data © TomTom",
            ("TomTom Commercial API Terms", "https://developer.tomtom.com/terms-and-conditions"),
            CommercialUse::Unknown,
            Some(
                "TomTom relative traffic flow vector tiles proxied with daily budget governor. Requires a TomTom API key.",
            ),
            vec![SecretKey::TomTomApiKey],
        ),
        source(
            OPENFREEMAP,
            "OpenFreeMap / OpenMapTiles",
            "https://openfreemap.org/",
            "Vector tiles: OpenFreeMap © OpenMapTiles; data © OpenStreetMap contributors (ODbL 1.0)",
            ("Open Database License (ODbL) 1.0", "https://opendatacommons.org/licenses/odbl/1-0/"),
            CommercialUse::Allowed,
            Some("OpenFreeMap public vector tiles for road transportation geometry matching."),
        ),
        source_with_secrets(
            OPENSKY,
            "The OpenSky Network",
            "https://opensky-network.org/",
            "Aircraft data: The OpenSky Network (opensky-network.org)",
            (
                "OpenSky Terms of Use (non-commercial)",
                "https://opensky-network.org/about/terms-of-use",
            ),
            CommercialUse::NonCommercialOnly,
            Some(
                "Evaluation and non-commercial aircraft feed with daily credit governor. Commercial use requires an OpenSky commercial licence.",
            ),
            vec![SecretKey::OpenSkyClientId, SecretKey::OpenSkyClientSecret],
        ),
        source(
            ADS_B_DB,
            "adsbdb",
            "https://www.adsbdb.com/",
            "Aircraft type and route data: adsbdb (adsbdb.com)",
            ("adsbdb API Terms", "https://www.adsbdb.com/"),
            CommercialUse::RequiresAgreement,
            Some(
                "Community aircraft route and type enrichment cache. Route redistribution requires publisher permission.",
            ),
        ),
        source(
            TELEGEOGRAPHY,
            "TeleGeography Submarine Cable Map",
            "https://www.submarinecablemap.com/",
            "Data: TeleGeography (CC BY-NC-SA 3.0)",
            (
                "Creative Commons Attribution-NonCommercial-ShareAlike 3.0 Unported",
                "https://creativecommons.org/licenses/by-nc-sa/3.0/",
            ),
            CommercialUse::NonCommercialOnly,
            Some(
                "Submarine telecommunication cables and landing points. Non-commercial and share-alike terms apply.",
            ),
        ),
        source(
            NOAA_NOWCOAST,
            "NOAA nowCOAST (Weather Radar & Hazards)",
            "https://nowcoast.noaa.gov/",
            "Weather radar & imagery: NOAA/NWS nowCOAST (public domain)",
            ("Public domain (U.S. Government work)", "https://www.weather.gov/disclaimer"),
            CommercialUse::Allowed,
            Some("NOAA nowCOAST WMS weather radar tiles. Public domain; no endorsement implied."),
        ),
        source(
            OSM_INFRASTRUCTURE,
            "OpenStreetMap Infrastructure",
            "https://www.openstreetmap.org/",
            "Infrastructure data © OpenStreetMap contributors (ODbL 1.0)",
            ("Open Database License (ODbL) 1.0", "https://opendatacommons.org/licenses/odbl/1-0/"),
            CommercialUse::Allowed,
            Some(
                "Global infrastructure facilities including dams and datacenters from OpenStreetMap / Overture Maps.",
            ),
        ),
        source(
            OSM_MILITARY,
            "OpenStreetMap Military & Defense Installations",
            "https://www.openstreetmap.org/",
            "Military installation data © OpenStreetMap contributors (ODbL 1.0)",
            ("Open Database License (ODbL) 1.0", "https://opendatacommons.org/licenses/odbl/1-0/"),
            CommercialUse::Allowed,
            Some(
                "Curated defensive, airbase, naval base and military facility polygons via Overpass API.",
            ),
        ),
        source(
            NASA_CMR,
            "NASA Common Metadata Repository (CMR)",
            "https://cmr.earthdata.nasa.gov/",
            "Imagery search metadata: NASA Earth Science Data and Information System (ESDIS)",
            (
                "NASA open data policy",
                "https://www.earthdata.nasa.gov/engage/open-data-services-software-policies",
            ),
            CommercialUse::Allowed,
            Some("Satellite imagery granule search via NASA CMR STAC API."),
        ),
        source(
            NATURAL_EARTH_DIVISIONS,
            "Natural Earth Admin-1 States and Provinces",
            "https://www.naturalearthdata.com/",
            crate::providers::divisions::DIVISIONS_ATTRIBUTION,
            ("Public domain", "https://www.naturalearthdata.com/about/terms-of-use/"),
            CommercialUse::Allowed,
            Some(
                "Admin-1 divisions and polygons bundled offline for drill-down and territorial analysis.",
            ),
        ),
        source(
            GNSS_INTEGRITY,
            "GNSS Navigation Integrity & Interference Detection",
            "https://gpsjam.org/",
            GNSS_INTEGRITY_ATTRIBUTION,
            ("Open Database License 1.0", "https://opendatacommons.org/licenses/odbl/1-0/"),
            CommercialUse::Allowed,
            Some(
                "Anomaly classification derived from ADS-B aircraft reported NIC and NACp values binned into 0.5-degree cells.",
            ),
        ),
        source(
            ECCC_AQHI,
            "Environment and Climate Change Canada (AQHI)",
            "https://eccc-msc.github.io/open-data/",
            ECCC_AQHI_ATTRIBUTION,
            (
                "Environment and Climate Change Canada Data Servers End-use Licence",
                "https://eccc-msc.github.io/open-data/licence/readme_en/",
            ),
            CommercialUse::Allowed,
            Some("Canadian Air Quality Health Index (AQHI) realtime station observations."),
        ),
        source(
            AIR_QUALITY,
            "Environment and Climate Change Canada (AQHI)",
            "https://eccc-msc.github.io/open-data/",
            ECCC_AQHI_ATTRIBUTION,
            (
                "Environment and Climate Change Canada Data Servers End-use Licence",
                "https://eccc-msc.github.io/open-data/licence/readme_en/",
            ),
            CommercialUse::Allowed,
            Some("Canadian Air Quality Health Index (AQHI) realtime station observations."),
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
