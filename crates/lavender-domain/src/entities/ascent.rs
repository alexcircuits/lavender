//! Simulated launch ascent trajectories. Purely synthetic: these records are computed
//! locally from a launch site and target-orbit parameters (see `lavender-geo`'s ascent
//! module), never observed or fetched, so they carry no upstream attribution.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::GeoPoint;

/// A milestone along a simulated ascent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AscentEventKind {
    Liftoff,
    /// First commanded pitch change after the vertical rise (start of the gravity turn).
    PitchOver,
    /// Greatest dynamic pressure of the ascent.
    MaxQ,
    /// First-stage main engine cutoff at propellant depletion.
    Meco,
    StageSeparation,
    SecondStageIgnition,
    /// Thrust cutoff at the target orbit condition; `reached_insertion` confirms it.
    OrbitalInsertion,
}

/// One mission milestone with its mission time in seconds after liftoff.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AscentEvent {
    pub time_s: f64,
    pub kind: AscentEventKind,
}

/// One sampled point of a simulated ascent.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AscentSample {
    /// Seconds after liftoff.
    pub time_s: f64,
    /// Sub-vehicle ground position; `altM` is the altitude above the mean-Earth-radius
    /// sphere the simulation integrates on.
    pub position: GeoPoint,
    /// Inertial speed magnitude in the launch plane (Earth rotation is not included).
    pub speed_mps: f64,
    /// Speed along the local vertical, metres per second (positive while climbing).
    pub vertical_speed_mps: f64,
    /// Great-circle ground distance travelled from the launch site, metres.
    pub downrange_m: f64,
    /// Total vehicle mass (stages, propellant remaining and payload), kilograms.
    pub mass_kg: f64,
}

/// A deterministic, simulated two-stage ascent from a launch site toward a target
/// circular orbit, sampled over mission time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AscentTrajectory {
    /// Name of the (illustrative) vehicle description that was simulated.
    pub vehicle: String,
    pub site: GeoPoint,
    /// Launch azimuth actually flown, degrees clockwise from north in [0, 360).
    pub azimuth_deg: f64,
    /// Target circular-orbit altitude the insertion condition aims at, metres.
    pub target_altitude_m: f64,
    /// Whether the cutoff condition was met before the propellant ran out.
    pub reached_insertion: bool,
    /// Sampled states in mission-time order, starting at liftoff.
    pub samples: Vec<AscentSample>,
    /// Mission milestones in time order.
    pub events: Vec<AscentEvent>,
}
