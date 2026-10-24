//! Normalised entities. Providers convert upstream payloads into these types; the
//! frontend renders only these.

mod aircraft;
mod bikeshare;
mod boundary;
mod cctv;
mod cyclone;
mod earthquake;
mod fire;
mod launch;
mod perimeter;
mod route;
mod satellite;
mod terrain;
mod traffic;
mod transit;
mod vessel;

pub use aircraft::{Aircraft, AircraftRouteEnrichment, AircraftTypeEnrichment, AirportInfo};
pub use bikeshare::{BikeshareStation, BikeshareSystem};
pub use boundary::{BoundaryClass, BoundaryFeature, BoundaryResponse};
pub use cctv::{CameraHealth, CameraSource, CctvDetection, CctvDetectionsResponse, CctvStreamInfo};
pub use cyclone::{
    Cyclone, CycloneConeGeometry, CycloneForecastPoint, CycloneGeometryStatus, CycloneMovement,
    CycloneTrackGeometry,
};
pub use earthquake::Earthquake;
pub use fire::{FireCell, FireCluster, FireDetection};
pub use launch::{Launch, LaunchPad};
pub use perimeter::{FirePerimeter, PerimeterDetail};
pub use route::{Route, RouteStep};
pub use satellite::SatelliteElements;
pub use terrain::{TerrainHeightsRequest, TerrainHeightsResponse, TerrainPointResult};
pub use traffic::{TrafficRoadSegment, TrafficStatus, TrafficTileResponse};
pub use transit::{TransitEpoch, TransitFeed, TransitTrail, TransitVehicle};
pub use vessel::{
    Vessel, VesselDimensions, VesselEta, VesselTrackResponse, VesselTrackSample, VesselVoyage,
    normalize_course, normalize_heading, normalize_speed,
};
