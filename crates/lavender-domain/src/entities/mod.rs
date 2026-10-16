//! Normalised entities. Providers convert upstream payloads into these types; the
//! frontend renders only these.

mod aircraft;
mod cyclone;
mod earthquake;
mod fire;
mod launch;
mod perimeter;
mod satellite;
mod vessel;

pub use aircraft::Aircraft;
pub use cyclone::{
    Cyclone, CycloneConeGeometry, CycloneForecastPoint, CycloneGeometryStatus, CycloneMovement,
    CycloneTrackGeometry,
};
pub use earthquake::Earthquake;
pub use fire::{FireCell, FireCluster, FireDetection};
pub use launch::{Launch, LaunchPad};
pub use perimeter::{FirePerimeter, PerimeterDetail};
pub use satellite::SatelliteElements;
pub use vessel::{
    Vessel, VesselDimensions, VesselEta, VesselTrackResponse, VesselTrackSample, VesselVoyage,
    normalize_course, normalize_heading, normalize_speed,
};
