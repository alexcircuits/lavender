//! Optics, Sensor Metrics & Cyber Sonar Kernel
//!
//! Provides zero-allocation, accelerated routines for:
//! - Ground Sample Distance (GSD) and NIIRS (National Imagery Interpretability Rating Scale)
//! - Off-Nadir Angle (ONA) and Solar Position / Elevation
//! - Tactical Cyber Sonar sweep gain and contact beam attenuation
//! - Scope aperture keyhole geometry and altitude-adaptive terminus alpha

const std = @import("std");
const math = std.math;

const PI: f64 = math.pi;
const DEG_TO_RAD: f64 = PI / 180.0;
const RAD_TO_DEG: f64 = 180.0 / PI;

/// Calculates Ground Sample Distance (GSD) in meters from camera ellipsoidal altitude.
pub fn calculateGsd(alt_m: f64) f64 {
    const alt = if (math.isNan(alt_m) or alt_m < 0.0) 0.0 else alt_m;
    return @max(0.01, alt * 0.000375);
}

/// Calculates NIIRS rating (0.0 to 9.0) using the simplified General Image Quality Equation (GIQE).
pub fn calculateNiirs(gsd_m: f64) f64 {
    const gsd = if (math.isNan(gsd_m) or gsd_m <= 0.0) 0.01 else gsd_m;
    const gsd_inches = gsd * 39.37;
    const log10_val = math.log10(gsd_inches);
    const niirs = 10.25 - 3.32 * log10_val;
    return math.clamp(niirs, 0.0, 9.0);
}

/// Calculates Off-Nadir Angle (ONA) in degrees (0 at nadir, up to 90+ toward horizon).
pub fn calculateOna(pitch_deg: f64) f64 {
    if (math.isNan(pitch_deg)) return 0.0;
    return @max(0.0, 90.0 + pitch_deg);
}

/// Normalizes an angle into [0, 360) range.
fn normalize360(deg: f64) f64 {
    const m = @mod(deg, 360.0);
    return if (m < 0.0) m + 360.0 else m;
}

/// Estimates Sun elevation in degrees above horizon for a given location and UTC timestamp.
pub fn estimateSunElevation(lat_deg: f64, lon_deg: f64, unix_sec: f64) f64 {
    const lat = math.clamp(lat_deg, -90.0, 90.0) * DEG_TO_RAD;
    const lon = lon_deg;

    // Julian Date
    const jd: f64 = 2440587.5 + unix_sec / 86400.0;
    // Julian centuries from J2000.0
    const t: f64 = (jd - 2451545.0) / 36525.0;

    // Geometric mean longitude of the Sun (deg)
    const l0 = normalize360(280.46646 + t * (36000.76983 + 0.0003032 * t));
    // Mean anomaly of the Sun (deg)
    const m = normalize360(357.52911 + t * (35999.05029 - 0.0001537 * t)) * DEG_TO_RAD;

    // Equation of Center (deg)
    const c = (1.914602 - t * (0.004817 + 0.000014 * t)) * @sin(m) +
        (0.019993 - 0.000101 * t) * @sin(2.0 * m) +
        0.000289 * @sin(3.0 * m);

    // Sun's true longitude (deg)
    const sun_lon = (l0 + c) * DEG_TO_RAD;

    // Obliquity of the ecliptic (deg)
    const eps = (23.439291 - 0.0130042 * t) * DEG_TO_RAD;

    // Declination of the Sun
    const sin_dec = @sin(eps) * @sin(sun_lon);
    const cos_dec = @sqrt(@max(0.0, 1.0 - sin_dec * sin_dec));

    // Greenwich Mean Sidereal Time (deg)
    const gmst = normalize360(280.46061837 + 360.98564736629 * (jd - 2451545.0));
    // Local Sidereal Time
    const lst = normalize360(gmst + lon) * DEG_TO_RAD;

    // Sun's Right Ascension
    const sun_ra = math.atan2(@cos(eps) * @sin(sun_lon), @cos(sun_lon));
    // Hour angle
    const ha = lst - sun_ra;

    // Elevation angle: sin(elev) = sin(lat)*sin(dec) + cos(lat)*cos(dec)*cos(ha)
    const sin_elev = @sin(lat) * sin_dec + @cos(lat) * cos_dec * @cos(ha);
    const clamped_sin = math.clamp(sin_elev, -1.0, 1.0);
    return math.asin(clamped_sin) * RAD_TO_DEG;
}

/// Evaluates tactical Cyber Sonar beam gain [0.0, 1.0] for a contact.
/// target_bearing_deg: bearing from observer to contact [0, 360)
/// sweep_heading_deg: instantaneous heading of rotating sonar sweep beam [0, 360)
/// sweep_width_deg: width of the active sweep sector (e.g. 24.0 deg)
/// feather_deg: soft edge feathering width (e.g. 10.0 deg)
pub fn calculateSonarSweepGain(
    target_bearing_deg: f64,
    sweep_heading_deg: f64,
    sweep_width_deg: f64,
    feather_deg: f64,
) f64 {
    const tb = normalize360(target_bearing_deg);
    const sh = normalize360(sweep_heading_deg);
    const width = if (sweep_width_deg <= 0.0) 24.0 else sweep_width_deg;
    const feather = if (feather_deg <= 0.0) 1.0 else feather_deg;

    // Shortest-arc angular difference in [-180, 180)
    const raw_delta = @mod(tb - sh + 540.0, 360.0) - 180.0;
    // We check if contact is trailing or inside sweep: delta between 0 and width
    const half_width = width * 0.5;
    const dist_from_center = @abs(raw_delta - half_width);

    if (dist_from_center <= half_width) {
        return 1.0;
    } else if (dist_from_center <= half_width + feather) {
        // Smoothstep hermite falloff
        const t = (dist_from_center - half_width) / feather;
        return 1.0 - t * t * (3.0 - 2.0 * t);
    } else {
        return 0.0;
    }
}

/// Scope aperture terminus alpha ramp across camera altitude.
/// At full globe (>= 10 Mm), alpha = 0.94 (faint stars in corners).
/// Ramps quickly to 1.0 (solid black) at or below 7 Mm.
/// Quantized to 0.01 increments to eliminate UI repaint chatter.
pub fn calculateScopeTerminusAlpha(alt_m: f64) f64 {
    const HIGH_ALT: f64 = 10_000_000.0;
    const LOW_ALT: f64 = 7_000_000.0;
    const ALPHA_HIGH: f64 = 0.94;
    const ALPHA_LOW: f64 = 1.0;

    if (alt_m >= HIGH_ALT) return ALPHA_HIGH;
    if (alt_m <= LOW_ALT) return ALPHA_LOW;

    const t = (alt_m - LOW_ALT) / (HIGH_ALT - LOW_ALT);
    const raw_alpha = ALPHA_LOW + (ALPHA_HIGH - ALPHA_LOW) * t;

    // Quantize to 0.01
    return @round(raw_alpha * 100.0) / 100.0;
}

/// Calculates keyhole circular aperture geometry [center_x, center_y, radius_px, feather_px].
pub fn calculateKeyholeGeometry(
    viewport_w: f64,
    viewport_h: f64,
    feather_ratio: f64,
) [4]f64 {
    const w = if (viewport_w <= 0.0) 1920.0 else viewport_w;
    const h = if (viewport_h <= 0.0) 1080.0 else viewport_h;
    const cx = w * 0.5;
    const cy = h * 0.5;
    const radius = @min(w, h) * 0.61 * 0.5;
    const ratio = math.clamp(feather_ratio, 0.0, 0.4);
    const feather = radius * ratio;

    return [_]f64{ cx, cy, radius, feather };
}
