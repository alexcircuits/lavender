//! Simulated launch ascent trajectories (LB-112).
//!
//! The standard two-degree-of-freedom ascent equations on a spherical Earth:
//! planar state is `(radius, downrange angle, speed, flight-path angle)` with
//! thrust steerable relative to the velocity vector. The gravity turn and the
//! orbital-insertion condition emerge from the physics — the flight path stops
//! bending once the centrifugal term balances gravity — so no trajectory
//! shaping is bolted on. Earth rotation is out of scope; the product is an
//! analyst-visible, deterministic ascent arc, not a guidance design. Produces
//! the `lavender_domain::AscentTrajectory` contract; purely synthetic, so it
//! carries no source attribution.

use lavender_domain::{AscentEvent, AscentEventKind, AscentSample, AscentTrajectory, GeoPoint};

use crate::geodesy;

const G0_MPS2: f64 = 9.806_65;
const EARTH_RADIUS_M: f64 = 6_371_000.0;
const EARTH_MU_M3_S2: f64 = 3.986_004_418e14;
/// Sea-level density and scale height of the exponential atmosphere.
const RHO0_KG_M3: f64 = 1.225;
const SCALE_HEIGHT_M: f64 = 8_500.0;

/// One stage's constant-thrust specification.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StageSpec {
    /// Sea-level thrust, newtons.
    pub thrust_n: f64,
    /// Specific impulse the mass flow is computed from, seconds.
    pub isp_s: f64,
    pub propellant_kg: f64,
    /// Stage mass after propellant depletion (dropped at separation).
    pub dry_mass_kg: f64,
}

/// An illustrative two-stage orbital-class vehicle, tuned so a nominal
/// low-earth-orbit mission reaches insertion with propellant margin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AscentVehicle {
    pub name: &'static str,
    pub stage1: StageSpec,
    pub stage2: StageSpec,
    pub payload_kg: f64,
    /// Drag area `Cd·A` in m², constant through flight.
    pub drag_area_m2: f64,
}

impl AscentVehicle {
    #[must_use]
    pub const fn illustrative() -> Self {
        Self {
            name: "Lavender illustrative two-stage",
            stage1: StageSpec {
                thrust_n: 7_600_000.0,
                isp_s: 282.0,
                propellant_kg: 420_000.0,
                dry_mass_kg: 27_000.0,
            },
            stage2: StageSpec {
                thrust_n: 2_200_000.0,
                isp_s: 340.0,
                propellant_kg: 70_000.0,
                dry_mass_kg: 4_000.0,
            },
            payload_kg: 8_000.0,
            drag_area_m2: 3.5,
        }
    }

    fn liftoff_mass_kg(&self) -> f64 {
        self.stage1.propellant_kg
            + self.stage1.dry_mass_kg
            + self.stage2.propellant_kg
            + self.stage2.dry_mass_kg
            + self.payload_kg
    }
}

/// Where and toward what the ascent flies.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AscentParams {
    pub site: GeoPoint,
    /// Launch azimuth actually flown, degrees clockwise from north.
    pub azimuth_deg: f64,
    /// Target circular-orbit altitude the cutoff condition aims at, metres.
    pub target_altitude_m: f64,
    /// Altitude at which the vehicle pitches over off vertical.
    pub pitch_over_altitude_m: f64,
}

impl Default for AscentParams {
    fn default() -> Self {
        Self {
            site: GeoPoint { lat: 28.572_9, lon: -80.649, alt_m: None },
            azimuth_deg: 95.0,
            target_altitude_m: 200_000.0,
            pitch_over_altitude_m: 1_200.0,
        }
    }
}

/// The launch azimuth that reaches a given orbital inclination from a latitude:
/// `sin(az) = cos(i) / cos(lat)`; None when the site cannot reach it.
#[must_use]
pub fn launch_azimuth_deg(site_lat_deg: f64, inclination_deg: f64) -> Option<f64> {
    let sin_az = (inclination_deg.to_radians()).cos() / (site_lat_deg.to_radians()).cos();
    if !(-1.0..=1.0).contains(&sin_az) {
        return None;
    }
    let az = sin_az.asin().to_degrees();
    Some(if az < 0.0 { 360.0 + az } else { az })
}

/// Propellant bookkeeping and stage transitions across the mission.
struct Staging {
    stage1: StageSpec,
    stage2: StageSpec,
    ignition_delay_s: f64,
    prop1: f64,
    prop2: f64,
    meco_time: Option<f64>,
    second_stage_ignited: bool,
}

impl Staging {
    fn new(vehicle: &AscentVehicle, ignition_delay_s: f64) -> Self {
        Self {
            stage1: vehicle.stage1,
            stage2: vehicle.stage2,
            ignition_delay_s,
            prop1: vehicle.stage1.propellant_kg,
            prop2: vehicle.stage2.propellant_kg,
            meco_time: None,
            second_stage_ignited: false,
        }
    }

    /// Whether the second stage is currently producing thrust.
    fn burning_stage2(&self) -> bool {
        self.meco_time.is_some() && self.prop2 > 0.0
    }

    /// Whether the mission has no propellant left after stage-one depletion.
    fn spent(&self) -> bool {
        self.meco_time.is_some() && self.prop2 <= 0.0
    }

    /// One time-step of stage logic: burns propellant, records MECO,
    /// separation and second-stage ignition events, adjusts `mass_kg`, and
    /// returns the thrust in newtons for the step.
    fn step(&mut self, dt_s: f64, t: f64, mass_kg: &mut f64, events: &mut Vec<AscentEvent>) -> f64 {
        let mdot1 = self.stage1.thrust_n / (self.stage1.isp_s * G0_MPS2);
        let mdot2 = self.stage2.thrust_n / (self.stage2.isp_s * G0_MPS2);
        let (thrust, mdot) = match self.meco_time {
            None => {
                if self.prop1 <= 0.0 {
                    self.meco_time = Some(t);
                    events.push(AscentEvent { time_s: t, kind: AscentEventKind::Meco });
                    events.push(AscentEvent { time_s: t, kind: AscentEventKind::StageSeparation });
                    *mass_kg -= self.stage1.dry_mass_kg;
                    (0.0, 0.0)
                } else {
                    (self.stage1.thrust_n, mdot1)
                }
            }
            Some(meco) => {
                let delay_passed = t >= meco + self.ignition_delay_s;
                if self.prop2 > 0.0 && delay_passed {
                    (self.stage2.thrust_n, mdot2)
                } else {
                    (0.0, 0.0)
                }
            }
        };
        if thrust > 0.0 {
            let (prop, other_stage) = if self.meco_time.is_none() {
                (&mut self.prop1, false)
            } else {
                (&mut self.prop2, true)
            };
            let consumed = (mdot * dt_s).min(*prop);
            *prop -= consumed;
            *mass_kg -= consumed;
            if other_stage && !self.second_stage_ignited {
                self.second_stage_ignited = true;
                events.push(AscentEvent { time_s: t, kind: AscentEventKind::SecondStageIgnition });
            }
        }
        thrust
    }
}

/// The commanded flight-path angle of the pitch program: vertical until
/// pitch-over, relaxing to 20 degrees by `KNEE_ALT_M` and to zero approaching
/// the target altitude. Real ascents are programmed, not passively turned; the
/// gravity-turn curvature term still acts underneath this command.
fn pitch_program_gamma(params: &AscentParams, alt_m: f64, pitch_over_done: bool) -> f64 {
    const PITCH_OVER_GAMMA_DEG: f64 = 85.0;
    const KNEE_ALT_M: f64 = 70_000.0;
    const KNEE_GAMMA_DEG: f64 = 20.0;
    if !pitch_over_done {
        return std::f64::consts::FRAC_PI_2;
    }
    let above = (alt_m - params.pitch_over_altitude_m).max(0.0);
    let to_knee = (PITCH_OVER_GAMMA_DEG - KNEE_GAMMA_DEG).to_radians()
        * (1.0 - (above / KNEE_ALT_M).min(1.0));
    let prog_alt_m = (params.target_altitude_m - 20_000.0).max(KNEE_ALT_M);
    let above_knee = (alt_m - KNEE_ALT_M).max(0.0);
    let to_zero =
        KNEE_GAMMA_DEG.to_radians() * (1.0 - (above_knee / (prog_alt_m - KNEE_ALT_M)).min(1.0));
    to_knee + to_zero.max(0.0)
}

struct PlanarState {
    /// Distance from the Earth's centre, metres.
    r_m: f64,
    /// Downrange central angle, radians.
    chi_rad: f64,
    /// Inertial speed magnitude in the launch plane, m/s.
    v_mps: f64,
    /// Flight-path angle above the local horizon, radians.
    gamma_rad: f64,
    mass_kg: f64,
}

/// Integrates the ascent and samples it every `SAMPLE_INTERVAL_S`.
#[must_use]
pub fn simulate_ascent(vehicle: &AscentVehicle, params: &AscentParams) -> AscentTrajectory {
    const DT_S: f64 = 0.25;
    const SAMPLE_INTERVAL_S: f64 = 2.0;
    const IGNITION_DELAY_S: f64 = 3.0;
    const MAX_MISSION_TIME_S: f64 = 700.0;
    /// Insertion: altitude met, path this close to horizontal, and speed this
    /// close to the local circular-orbit speed, while the stage still burns.
    const INSERTION_GAMMA_MAX_DEG: f64 = 8.0;
    const INSERTION_SPEED_FRACTION: f64 = 0.98;

    let mut staging = Staging::new(vehicle, IGNITION_DELAY_S);
    let mut st = PlanarState {
        r_m: EARTH_RADIUS_M,
        chi_rad: 0.0,
        v_mps: 0.0,
        gamma_rad: std::f64::consts::FRAC_PI_2,
        mass_kg: vehicle.liftoff_mass_kg(),
    };

    let mut samples: Vec<AscentSample> = Vec::new();
    let mut events: Vec<AscentEvent> =
        vec![AscentEvent { time_s: 0.0, kind: AscentEventKind::Liftoff }];
    let mut max_q = 0.0;
    let mut max_q_recorded = false;
    let mut pitch_over_done = false;
    let mut reached_insertion = false;
    let mut t = 0.0;
    let mut next_sample = 0.0;

    while t < MAX_MISSION_TIME_S {
        t += DT_S;
        let alt = st.r_m - EARTH_RADIUS_M;
        let thrust = staging.step(DT_S, t, &mut st.mass_kg, &mut events);

        // Pitch-over: command the first off-vertical flight-path angle.
        if !pitch_over_done && alt >= params.pitch_over_altitude_m {
            st.gamma_rad = 85.0_f64.to_radians();
            pitch_over_done = true;
            events.push(AscentEvent { time_s: t, kind: AscentEventKind::PitchOver });
        }

        let gamma_prog = pitch_program_gamma(params, alt, pitch_over_done);
        let delta_rad = if thrust > 0.0 && st.v_mps > 1.0 {
            (4.0 * (gamma_prog - st.gamma_rad)).clamp(-0.5, 0.5)
        } else {
            0.0
        };

        let g = EARTH_MU_M3_S2 / (st.r_m * st.r_m);
        let rho = RHO0_KG_M3 * (-alt.max(0.0) / SCALE_HEIGHT_M).exp();
        let drag = 0.5 * rho * st.v_mps * st.v_mps * vehicle.drag_area_m2;

        // MaxQ needs a real maximum: ignore liftoff transients.
        let q = 0.5 * rho * st.v_mps * st.v_mps;
        if q > max_q {
            max_q = q;
        } else if !max_q_recorded && max_q > 5_000.0 && q < max_q * 0.95 {
            max_q_recorded = true;
            events.push(AscentEvent { time_s: t, kind: AscentEventKind::MaxQ });
        }

        // The 2-DOF spherical ascent equations.
        let sin_g = st.gamma_rad.sin();
        let cos_g = st.gamma_rad.cos();
        let dv = (thrust - drag) / st.mass_kg - g * sin_g;
        let dgamma = if st.v_mps > 1.0 {
            (thrust * delta_rad.sin()) / (st.mass_kg * st.v_mps)
                + (st.v_mps / st.r_m - g / st.v_mps) * cos_g
        } else {
            0.0
        };
        st.v_mps = (st.v_mps + dv * DT_S).max(0.0);
        st.gamma_rad += dgamma * DT_S;
        st.r_m += st.v_mps * sin_g * DT_S;
        st.chi_rad += st.v_mps * cos_g * DT_S / st.r_m;
        // Never dive: the simulation's job ends if the path points down hard.
        st.gamma_rad = st.gamma_rad.max(-0.35);

        // Insertion cutoff: target altitude reached, path horizontal, speed at
        // the local circular-orbit value, while the stage still burns.
        let v_circ = (EARTH_MU_M3_S2 / st.r_m).sqrt();
        if alt >= 0.80 * params.target_altitude_m
            && st.gamma_rad.abs() <= INSERTION_GAMMA_MAX_DEG.to_radians()
            && st.v_mps >= INSERTION_SPEED_FRACTION * v_circ
            && staging.burning_stage2()
        {
            reached_insertion = true;
            events.push(AscentEvent { time_s: t, kind: AscentEventKind::OrbitalInsertion });
        }

        if t >= next_sample {
            next_sample += SAMPLE_INTERVAL_S;
            samples.push(sample_at(&params, t, &st, alt));
        }

        if reached_insertion || (staging.spent() && st.gamma_rad < -0.05) {
            break;
        }
    }

    if !max_q_recorded {
        events.push(AscentEvent { time_s: t, kind: AscentEventKind::MaxQ });
    }
    events.sort_by(|a, b| a.time_s.total_cmp(&b.time_s));

    AscentTrajectory {
        vehicle: vehicle.name.to_string(),
        site: params.site,
        azimuth_deg: params.azimuth_deg,
        target_altitude_m: params.target_altitude_m,
        reached_insertion,
        samples,
        events,
    }
}

/// One output sample from the planar state: ground track along the azimuth,
/// altitude above the sphere, speeds and downrange arc.
fn sample_at(params: &AscentParams, t: f64, st: &PlanarState, alt_m: f64) -> AscentSample {
    let downrange_m = st.chi_rad * EARTH_RADIUS_M;
    let ground = geodesy::destination(&params.site, params.azimuth_deg, downrange_m);
    AscentSample {
        time_s: t,
        position: GeoPoint { lat: ground.lat, lon: ground.lon, alt_m: Some(alt_m) },
        speed_mps: st.v_mps,
        vertical_speed_mps: st.v_mps * st.gamma_rad.sin(),
        downrange_m,
        mass_kg: st.mass_kg,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nominal() -> (AscentVehicle, AscentParams) {
        (AscentVehicle::illustrative(), AscentParams::default())
    }

    #[test]
    fn nominal_ascent_reaches_insertion_with_ordered_events() {
        let (v, p) = nominal();
        let t = simulate_ascent(&v, &p);
        assert!(t.reached_insertion, "events: {:?}", t.events);
        let kinds: Vec<_> = t.events.iter().map(|e| e.kind).collect();
        let order = [
            AscentEventKind::Liftoff,
            AscentEventKind::PitchOver,
            AscentEventKind::MaxQ,
            AscentEventKind::Meco,
            AscentEventKind::StageSeparation,
            AscentEventKind::SecondStageIgnition,
            AscentEventKind::OrbitalInsertion,
        ];
        for k in order {
            assert!(kinds.contains(&k), "missing {k:?} in {kinds:?}");
        }
        for w in t.events.windows(2) {
            assert!(w[0].time_s <= w[1].time_s);
        }
    }

    #[test]
    fn geometry_is_sane() {
        let (v, p) = nominal();
        let t = simulate_ascent(&v, &p);
        for w in t.samples.windows(2) {
            assert!(w[1].downrange_m >= w[0].downrange_m - 1e-6, "downrange must not retreat");
        }
        assert!(t.samples.iter().all(|s| s.position.lat.abs() <= 90.0));
        assert!(
            t.samples.iter().all(|s| s.position.alt_m.unwrap_or(0.0) < p.target_altitude_m * 1.5)
        );
        let last = t.samples.last().expect("samples");
        assert!(last.speed_mps > 6_500.0 && last.speed_mps < 9_500.0, "speed {}", last.speed_mps);
    }

    #[test]
    fn deterministic() {
        let (v, p) = nominal();
        assert_eq!(simulate_ascent(&v, &p), simulate_ascent(&v, &p));
    }

    #[test]
    fn unreachable_inclination_is_refused() {
        assert!(launch_azimuth_deg(0.0, 90.0).is_some());
        assert!(launch_azimuth_deg(28.0, 140.0).is_some());
        assert!(launch_azimuth_deg(60.0, 170.0).is_none());
    }
}
