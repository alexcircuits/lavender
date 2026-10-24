//! SGP4 Satellite Orbit Propagator Kernel (Vallado 2006 Reference)
//!
//! Provides bare-metal orbit propagation from two-line or OMM mean elements
//! into True Equator Mean Equinox (TEME) coordinates, Earth-Centered Earth-Fixed
//! (ECEF) coordinates, and WGS84 sub-satellite geodetic positions.

const std = @import("std");

pub const WGS84_A_KM: f64 = 6378.137;
pub const WGS84_F: f64 = 1.0 / 298.257223563;
pub const WGS84_E2: f64 = WGS84_F * (2.0 - WGS84_F);
pub const WGS84_B_KM: f64 = WGS84_A_KM * (1.0 - WGS84_F);
pub const WGS84_EP2: f64 = (WGS84_A_KM * WGS84_A_KM - WGS84_B_KM * WGS84_B_KM) / (WGS84_B_KM * WGS84_B_KM);

pub const WGS72_A_KM: f64 = 6378.135;
pub const WGS72_XKE: f64 = 0.0743669161331734132;
pub const WGS72_J2: f64 = 0.001082616;
pub const WGS72_J3: f64 = -0.00000253881;
pub const WGS72_J4: f64 = -0.00000165597;
pub const WGS72_J3OJ2: f64 = WGS72_J3 / WGS72_J2;

pub const X2O3: f64 = 2.0 / 3.0;

pub const Sgp4Error = error{
    EccentricityOutOfRange,
    MeanMotionNonPositive,
    OrbitDecayed,
    PropagationDiverged,
};

pub const GravitationalConstants = struct {
    a_km: f64 = WGS72_A_KM,
    xke: f64 = WGS72_XKE,
    j2: f64 = WGS72_J2,
    j3: f64 = WGS72_J3,
    j4: f64 = WGS72_J4,
    j3oj2: f64 = WGS72_J3OJ2,
};

pub const SatelliteElements = struct {
    epoch_days: f64,
    bstar: f64,
    inclination_rad: f64,
    raan_rad: f64,
    eccentricity: f64,
    arg_perigee_rad: f64,
    mean_anomaly_rad: f64,
    mean_motion_rad_min: f64, // Kozai mean motion in rad/min
};

pub const SatRec = struct {
    bstar: f64,
    inclo: f64,
    nodeo: f64,
    ecco: f64,
    argpo: f64,
    mo: f64,
    no: f64, // Un-kozai Brouwer mean motion

    // Coefficients
    a: f64,
    alta: f64,
    altp: f64,
    con41: f64,
    con42: f64,
    x1mth2: f64,
    x7thm1: f64,
    aycof: f64,
    xlcof: f64,
    omgcof: f64,
    xmcof: f64,
    delmo: f64,
    sinmao: f64,
    eta: f64,

    // Secular rates
    mdot: f64,
    argpdot: f64,
    nodedot: f64,
    nodecf: f64,

    // Drag terms
    cc1: f64,
    cc4: f64,
    cc5: f64,
    d2: f64,
    d3: f64,
    d4: f64,
    t2cof: f64,
    t3cof: f64,
    t4cof: f64,
    t5cof: f64,

    con: GravitationalConstants,
};

pub const Vec3 = struct {
    x: f64,
    y: f64,
    z: f64,
};

pub const Geodetic = struct {
    latitude_deg: f64,
    longitude_deg: f64,
    altitude_m: f64,
};

/// Computes Greenwich Mean Sidereal Time (GMST) in radians from Julian Date.
pub fn gmstFromJd(jd: f64) f64 {
    const tut1 = (jd - 2451545.0) / 36525.0;
    var gmst_sec = 67310.54841 + (876600.0 * 3600.0 + 8640184.812866) * tut1 +
        0.093104 * tut1 * tut1 - 6.2e-6 * tut1 * tut1 * tut1;

    const two_pi = 2.0 * std.math.pi;
    gmst_sec = @mod(gmst_sec, 86400.0);
    if (gmst_sec < 0.0) gmst_sec += 86400.0;

    const gmst_rad = (gmst_sec * two_pi) / 86400.0;
    return @mod(gmst_rad, two_pi);
}

/// Initializes SGP4 coefficients conforming strictly to Vallado's sgp4init and initl.
pub fn initSatrec(el: SatelliteElements, con: GravitationalConstants) Sgp4Error!SatRec {
    if (el.eccentricity < 0.0 or el.eccentricity >= 0.999) return error.EccentricityOutOfRange;
    if (el.mean_motion_rad_min <= 0.0) return error.MeanMotionNonPositive;

    const eccsq = el.eccentricity * el.eccentricity;
    const omeosq = 1.0 - eccsq;
    const rteosq = @sqrt(omeosq);
    const cosio = @cos(el.inclination_rad);
    const cosio2 = cosio * cosio;
    const cosio4 = cosio2 * cosio2;
    const sinio = @sin(el.inclination_rad);

    // Un-kozai Brouwer mean motion
    const ak = std.math.pow(f64, con.xke / el.mean_motion_rad_min, X2O3);
    const d1 = (0.75 * con.j2 * (3.0 * cosio2 - 1.0)) / (rteosq * omeosq);
    var del_prime = d1 / (ak * ak);
    const adel = ak * (1.0 - del_prime * del_prime - del_prime * (1.0 / 3.0 + (134.0 * del_prime * del_prime) / 81.0));
    del_prime = d1 / (adel * adel);
    const no = el.mean_motion_rad_min / (1.0 + del_prime);
    const ao = std.math.pow(f64, con.xke / no, X2O3);

    const po = ao * omeosq;
    const con42 = 1.0 - 5.0 * cosio2;
    const con41 = -con42 - cosio2 - cosio2;
    const posq = po * po;
    const rp = ao * (1.0 - el.eccentricity);

    const ss = 78.0 / con.a_km + 1.0;
    const qzms2ttemp = (120.0 - 78.0) / con.a_km;
    const qzms2t = qzms2ttemp * qzms2ttemp * qzms2ttemp * qzms2ttemp;

    var sfour = ss;
    var qzms24 = qzms2t;
    const perige = (rp - 1.0) * con.a_km;

    if (perige < 156.0) {
        sfour = perige - 78.0;
        if (perige < 98.0) sfour = 20.0;
        const qzms24temp = (120.0 - sfour) / con.a_km;
        qzms24 = qzms24temp * qzms24temp * qzms24temp * qzms24temp;
        sfour = sfour / con.a_km + 1.0;
    }

    const pinvsq = 1.0 / posq;
    const tsi = 1.0 / (ao - sfour);
    const eta = ao * el.eccentricity * tsi;
    const etasq = eta * eta;
    const eeta = el.eccentricity * eta;
    const psisq = @abs(1.0 - etasq);
    const coef = qzms24 * std.math.pow(f64, tsi, 4.0);
    const coef1 = coef / std.math.pow(f64, psisq, 3.5);

    const cc2 = coef1 * no * (ao * (1.0 + 1.5 * etasq + eeta * (4.0 + etasq)) +
        ((0.375 * con.j2 * tsi) / psisq) * con41 * (8.0 + 3.0 * etasq * (8.0 + etasq)));
    const cc1 = el.bstar * cc2;

    var cc3: f64 = 0.0;
    if (el.eccentricity > 1e-4) {
        cc3 = (-2.0 * coef * tsi * con.j3oj2 * no * sinio) / el.eccentricity;
    }

    const x1mth2 = 1.0 - cosio2;
    const cc4 = 2.0 * no * coef1 * ao * omeosq * (eta * (2.0 + 0.5 * etasq) +
        el.eccentricity * (0.5 + 2.0 * etasq) -
        ((con.j2 * tsi) / (ao * psisq)) * (-3.0 * con41 * (1.0 - 2.0 * eeta + etasq * (1.5 - 0.5 * eeta)) +
        0.75 * x1mth2 * (2.0 * etasq - eeta * (1.0 + etasq)) * @cos(2.0 * el.arg_perigee_rad)));

    const cc5 = 2.0 * coef1 * ao * omeosq * (1.0 + 2.75 * (etasq + eeta) + eeta * etasq);

    const temp1 = 1.5 * con.j2 * pinvsq * no;
    const temp2 = 0.5 * temp1 * con.j2 * pinvsq;
    const temp3 = -0.46875 * con.j4 * pinvsq * pinvsq * no;

    const mdot = no + 0.5 * temp1 * rteosq * con41 + 0.0625 * temp2 * rteosq * (13.0 - 78.0 * cosio2 + 137.0 * cosio4);
    const argpdot = -0.5 * temp1 * con42 + 0.0625 * temp2 * (7.0 - 114.0 * cosio2 + 395.0 * cosio4) +
        temp3 * (3.0 - 36.0 * cosio2 + 49.0 * cosio4);
    const xhdot1 = -temp1 * cosio;
    const nodedot = xhdot1 + (0.5 * temp2 * (4.0 - 19.0 * cosio2) + 2.0 * temp3 * (3.0 - 7.0 * cosio2)) * cosio;

    const omgcof = el.bstar * cc3 * @cos(el.arg_perigee_rad);
    const xmcof = if (el.eccentricity > 1e-4) (-X2O3 * coef * el.bstar) / eeta else 0.0;
    const nodecf = 3.5 * omeosq * xhdot1 * cc1;
    const t2cof = 1.5 * cc1;

    const temp4 = 1.5e-12;
    const xlcof = if (@abs(cosio + 1.0) > 1.5e-12)
        (-0.25 * con.j3oj2 * sinio * (3.0 + 5.0 * cosio)) / (1.0 + cosio)
    else
        (-0.25 * con.j3oj2 * sinio * (3.0 + 5.0 * cosio)) / temp4;

    const aycof = -0.5 * con.j3oj2 * sinio;
    const delmotemp = 1.0 + eta * @cos(el.mean_anomaly_rad);
    const delmo = delmotemp * delmotemp * delmotemp;
    const sinmao = @sin(el.mean_anomaly_rad);
    const x7thm1 = 7.0 * cosio2 - 1.0;

    return SatRec{
        .bstar = el.bstar,
        .inclo = el.inclination_rad,
        .nodeo = el.raan_rad,
        .ecco = el.eccentricity,
        .argpo = el.arg_perigee_rad,
        .mo = el.mean_anomaly_rad,
        .no = no,
        .a = ao,
        .alta = ao * (1.0 + el.eccentricity) - 1.0,
        .altp = rp - 1.0,
        .con41 = con41,
        .con42 = con42,
        .x1mth2 = x1mth2,
        .x7thm1 = x7thm1,
        .aycof = aycof,
        .xlcof = xlcof,
        .omgcof = omgcof,
        .xmcof = xmcof,
        .delmo = delmo,
        .sinmao = sinmao,
        .eta = eta,
        .mdot = mdot,
        .argpdot = argpdot,
        .nodedot = nodedot,
        .nodecf = nodecf,
        .cc1 = cc1,
        .cc4 = cc4,
        .cc5 = cc5,
        .d2 = 4.0 * ao * tsi * cc1 * cc1,
        .d3 = (4.0 / 3.0) * ao * tsi * (17.0 * ao + sfour) * cc1 * cc1 * cc1,
        .d4 = (2.0 / 3.0) * ao * tsi * (221.0 * ao + 31.0 * sfour) * cc1 * cc1 * cc1 * cc1,
        .t2cof = t2cof,
        .t3cof = cc1 * cc1,
        .t4cof = 0.25 * (3.0 * cc1 * cc1 * cc1),
        .t5cof = 0.2 * (3.0 * cc1 * cc1 * cc1 * cc1),
        .con = con,
    };
}

/// Propagates satellite position at `tsince` (minutes since epoch) to TEME coordinates (km).
pub fn propagateTeme(sat: *const SatRec, tsince: f64) Sgp4Error!Vec3 {
    const two_pi = 2.0 * std.math.pi;

    // Secular gravity and drag update
    const xmdf = sat.mo + sat.mdot * tsince;
    const argpdf = sat.argpo + sat.argpdot * tsince;
    const nodedf = sat.nodeo + sat.nodedot * tsince;

    var argpm = argpdf;
    var mm = xmdf;
    const t2 = tsince * tsince;
    const nodem = nodedf + sat.nodecf * t2;
    var tempa = 1.0 - sat.cc1 * tsince;
    var tempe = sat.bstar * sat.cc4 * tsince;
    var templ = sat.t2cof * t2;

    const delomg = sat.omgcof * tsince;
    const delmtemp = 1.0 + sat.eta * @cos(xmdf);
    const delm = sat.xmcof * (delmtemp * delmtemp * delmtemp - sat.delmo);
    const temp = delomg + delm;
    mm = xmdf + temp;
    argpm = argpdf - temp;
    const t3 = t2 * tsince;
    const t4 = t3 * tsince;
    tempa = tempa - sat.d2 * t2 - sat.d3 * t3 - sat.d4 * t4;
    tempe += sat.bstar * sat.cc5 * (@sin(mm) - sat.sinmao);
    templ = templ + sat.t3cof * t3 + t4 * (sat.t4cof + tsince * sat.t5cof);

    var nm = sat.no;
    var em = sat.ecco - tempe;
    const inclm = sat.inclo;

    if (nm <= 0.0) return error.MeanMotionNonPositive;
    const am = std.math.pow(f64, sat.con.xke / nm, X2O3) * tempa * tempa;
    nm = sat.con.xke / std.math.pow(f64, am, 1.5);

    if (em >= 1.0 or em < -0.001) return error.EccentricityOutOfRange;
    if (em < 1.0e-6) em = 1.0e-6;

    mm += sat.no * templ;
    var xlm = mm + argpm + nodem;
    xlm = @mod(xlm, two_pi);
    argpm = @mod(argpm, two_pi);
    const nodep = @mod(nodem, two_pi);
    const argpp = argpm;
    const ep = em;
    const mp = @mod(xlm - argpm - nodem, two_pi);

    const sinip = @sin(inclm);
    const cosip = @cos(inclm);

    const axnl = ep * @cos(argpp);
    const temp_a = 1.0 / (am * (1.0 - ep * ep));
    const aynl = ep * @sin(argpp) + temp_a * sat.aycof;
    const xl = mp + argpp + nodep + temp_a * sat.xlcof * axnl;

    // Kepler's equation: u = (xl - nodep) % twoPi
    var u = @mod(xl - nodep, two_pi);
    if (u < 0.0) u += two_pi;

    var eo1 = u;
    var tem5: f64 = 9999.9;
    var ktr: usize = 1;

    while (@abs(tem5) >= 1.0e-12 and ktr <= 10) : (ktr += 1) {
        const sineo1 = @sin(eo1);
        const coseo1 = @cos(eo1);
        tem5 = 1.0 - coseo1 * axnl - sineo1 * aynl;
        tem5 = (u - aynl * coseo1 + axnl * sineo1 - eo1) / tem5;
        if (@abs(tem5) >= 0.95) {
            tem5 = if (tem5 > 0.0) 0.95 else -0.95;
        }
        eo1 += tem5;
    }

    const coseo1 = @cos(eo1);
    const sineo1 = @sin(eo1);

    const ecose = axnl * coseo1 + aynl * sineo1;
    const esine = axnl * sineo1 - aynl * coseo1;
    const el2 = axnl * axnl + aynl * aynl;
    const pl = am * (1.0 - el2);
    if (pl < 0.0) return error.PropagationDiverged;

    const rl = am * (1.0 - ecose);
    const betal = @sqrt(1.0 - el2);
    const temp_b = esine / (1.0 + betal);
    const sinu = (am / rl) * (sineo1 - aynl - axnl * temp_b);
    const cosu = (am / rl) * (coseo1 - axnl + aynl * temp_b);
    var su = std.math.atan2(sinu, cosu);
    const sin2u = (cosu + cosu) * sinu;
    const cos2u = 1.0 - 2.0 * sinu * sinu;

    const temp_c = 1.0 / pl;
    const temp1_c = 0.5 * sat.con.j2 * temp_c;
    const temp2_c = temp1_c * temp_c;

    const mrt = rl * (1.0 - 1.5 * temp2_c * betal * sat.con41) + 0.5 * temp1_c * sat.x1mth2 * cos2u;
    if (mrt < 1.0) return error.OrbitDecayed;

    su -= 0.25 * temp2_c * sat.x7thm1 * sin2u;
    const xnode = nodep + 1.5 * temp2_c * cosip * sin2u;
    const xinc = inclm + 1.5 * temp2_c * cosip * sinip * cos2u;

    const sinsu = @sin(su);
    const cossu = @cos(su);
    const snod = @sin(xnode);
    const cnod = @cos(xnode);
    const sini = @sin(xinc);
    const cosi = @cos(xinc);

    const xmx = -snod * cosi;
    const xmy = cnod * cosi;
    const ux = xmx * sinsu + cnod * cossu;
    const uy = xmy * sinsu + snod * cossu;
    const uz = sini * sinsu;

    return Vec3{
        .x = mrt * ux * sat.con.a_km,
        .y = mrt * uy * sat.con.a_km,
        .z = mrt * uz * sat.con.a_km,
    };
}

/// Rotates TEME position vector into Earth-Centered Earth-Fixed (ECEF) frame by angle `gmst`.
pub fn temeToEcf(teme: Vec3, gmst_rad: f64) Vec3 {
    const cosg = @cos(gmst_rad);
    const sing = @sin(gmst_rad);

    return Vec3{
        .x = cosg * teme.x + sing * teme.y,
        .y = -sing * teme.x + cosg * teme.y,
        .z = teme.z,
    };
}

/// Converts ECEF position in kilometers to WGS84 Geodetic coordinates (lat, lon, alt).
pub fn ecfToGeodetic(ecf: Vec3) Geodetic {
    const x = ecf.x;
    const y = ecf.y;
    const z = ecf.z;

    const p = @sqrt(x * x + y * y);
    const lon_rad = std.math.atan2(y, x);

    // Bowring's method for geodetic latitude
    const theta = std.math.atan2(z * WGS84_A_KM, p * WGS84_B_KM);
    const sin_theta = @sin(theta);
    const cos_theta = @cos(theta);

    const top = z + WGS84_EP2 * WGS84_B_KM * sin_theta * sin_theta * sin_theta;
    const bottom = p - WGS84_E2 * WGS84_A_KM * cos_theta * cos_theta * cos_theta;
    const lat_rad = std.math.atan2(top, bottom);

    const sin_lat = @sin(lat_rad);
    const n = WGS84_A_KM / @sqrt(1.0 - WGS84_E2 * sin_lat * sin_lat);
    const alt_km = (p / @cos(lat_rad)) - n;

    const rad_to_deg = 180.0 / std.math.pi;
    var lon_deg = lon_rad * rad_to_deg;
    if (lon_deg > 180.0) lon_deg -= 360.0;
    if (lon_deg < -180.0) lon_deg += 360.0;

    return Geodetic{
        .latitude_deg = lat_rad * rad_to_deg,
        .longitude_deg = lon_deg,
        .altitude_m = alt_km * 1000.0,
    };
}
