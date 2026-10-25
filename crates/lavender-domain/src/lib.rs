//! Lavender's domain contract.
//!
//! Every type that crosses a transport (HTTP, WebSocket, Tauri IPC) lives here, derives
//! `serde` with camelCase fields, and derives [`ts_rs::TS`] so the frontend's TypeScript
//! types are generated from these definitions (ADR 0007). Constructors and deserialisers
//! validate, so a value of these types is always in range.

pub mod catalog;
pub mod entities;
pub mod error;
pub mod feed;
pub mod geo;
pub mod search;
pub mod source;
pub mod status;
pub mod stream;
pub mod text;
pub mod track;
pub mod wind;

pub use catalog::{
    Availability, Catalog, ImageryDescriptor, ImageryKind, LayerDescriptor, LayerFamily,
    ProviderScope,
};
pub use entities::{
    AirQualityBand, AirQualityReading, AirQualitySnapshot, Aircraft, AircraftRouteEnrichment,
    AircraftTypeEnrichment, AirportInfo, AlertEvent, AlertKind, CameraHealth, CameraSource,
    CctvStreamInfo, Cyclone, CycloneConeGeometry, CycloneForecastPoint, CycloneGeometryStatus,
    CycloneMovement, CycloneTrackGeometry, Earthquake, FireCell, FireCluster, FireDetection,
    Geofence, GeofenceRule, GnssCell, GnssLevel, GnssProvenance, GnssSnapshot, Launch, LaunchPad,
    SatelliteElements, SpatialFilter, SpatialHit, SpatialSearchRequest, SpatialSearchResponse,
    Vessel, VesselDimensions, VesselEta, VesselTrackResponse, VesselTrackSample, VesselVoyage,
};
pub use entities::{AscentEvent, AscentEventKind, AscentSample, AscentTrajectory};
pub use entities::{BikeshareStation, BikeshareSystem};
pub use entities::{BoundaryClass, BoundaryFeature, BoundaryResponse};
pub use entities::{CanonicalEntity, EntityIdentifier, EntityKind, EntityLink};
pub use entities::{FirePerimeter, PerimeterDetail};
pub use entities::{Route, RouteStep};
pub use entities::{SavedSearch, SavedSearchMatch};
pub use entities::{TerrainHeightsRequest, TerrainHeightsResponse, TerrainPointResult};
pub use entities::{TrafficRoadSegment, TrafficStatus, TrafficTileResponse};
pub use entities::{TransitEpoch, TransitFeed, TransitTrail, TransitVehicle};
pub use error::{ApiError, DomainError, ErrorCode};
pub use feed::{Coverage, FeedFilters, FeedId, FeedItems, FeedQuery, FeedSnapshot, Freshness};
pub use geo::{BBox, GeoPoint};
pub use search::{CoordinateHit, EntityHit, PlaceHit, SearchQuery, SearchResults};
pub use source::{CommercialUse, Licence, LicenceMode, Privacy, SourceInfo};
pub use status::{FeedStatus, HealthState, ProviderHealth, SystemStatus};
pub use stream::{ClientFrame, ServerFrame};
pub use track::{
    AircraftTrack, SatellitePass, SatellitePassRequest, SatellitePasses, SatelliteTrack,
    SatelliteTrackRequest, TrackPoint,
};
pub use wind::{
    WeatherOverlay, WeatherScalarMetadata, WindCycle, WindGridDef, WindManifest, WindModel,
    WindStatus,
};

/// Timestamps on the wire are RFC 3339 strings in UTC.
pub use jiff::Timestamp;
