/**
 * Celestial reference ring model (LB-160): the pure, testable core of the
 * ecliptic horizon ported from upstream `legacy/src/celestialRing.js` — the
 * hysteresis full-globe keyhole test and the stable screen-angle projection
 * for the Sun and Moon markers. Rendering (canvas overlay, Cesium planetary
 * positions) lives with the globe; this module has no Cesium imports so the
 * behaviour can be tested directly.
 */

/** Clearance (px) the whole Earth disc needs before the ring appears. */
export const GLOBE_ENTER_CLEARANCE_PX = 24;
/** Clearance (px) below which a visible ring hides: exiting is easier than entering. */
export const GLOBE_EXIT_CLEARANCE_PX = 12;
/** Plane-length below which a celestial vector points into/out of the camera. */
export const CELESTIAL_PLANE_EPSILON = 0.045;

export interface KeyholeGeometry {
  readonly earthCenterX: number;
  readonly earthCenterY: number;
  readonly earthRadius: number;
  readonly keyholeCenterX: number;
  readonly keyholeCenterY: number;
  readonly keyholeRadius: number;
}

export function normalizeAngle(angle: number): number {
  if (!Number.isFinite(angle)) return 0;
  const wrapped = angle % (2 * Math.PI);
  return wrapped < 0 ? wrapped + 2 * Math.PI : wrapped;
}

/** Shortest signed distance between two angles, radians in [-π, π]. */
export function circularAngleDistance(a: number, b: number): number {
  const delta = normalizeAngle(a) - normalizeAngle(b);
  return ((delta + 3 * Math.PI) % (2 * Math.PI)) - Math.PI;
}

export interface ScreenAngle {
  readonly angle: number;
  readonly opacity: number;
  /** False while the vector is too aligned with the camera axis to trust. */
  readonly stable: boolean;
}

/**
 * Screen-space bearing of a celestial body from its camera-plane components.
 * Keeps the prior bearing (faded) when the vector points almost directly
 * into/out of the camera, so markers never spin wildly at the pole of the
 * camera plane.
 */
export function celestialScreenAngle(
  rightComponent: number,
  upComponent: number,
  lastAngle = 0,
): ScreenAngle {
  const planeLength = Math.hypot(rightComponent, upComponent);
  if (!Number.isFinite(planeLength) || planeLength < CELESTIAL_PLANE_EPSILON) {
    return {
      angle: normalizeAngle(Number.isFinite(lastAngle) ? lastAngle : 0),
      opacity: Math.min(1, Math.max(0, planeLength / CELESTIAL_PLANE_EPSILON)),
      stable: false,
    };
  }
  return {
    angle: normalizeAngle(Math.atan2(-upComponent, rightComponent)),
    opacity: Math.min(1, Math.max(0.28, (planeLength - CELESTIAL_PLANE_EPSILON) / 0.16)),
    stable: true,
  };
}

/**
 * Whether the projected Earth disc sits fully inside the keyhole. The
 * different enter/exit clearances give hysteresis while zooming, so the ring
 * does not flicker at the threshold.
 */
export function isFullGlobeInsideKeyhole(
  geometry: KeyholeGeometry | null | undefined,
  wasVisible = false,
): boolean {
  if (!geometry) return false;
  const { earthCenterX, earthCenterY, earthRadius, keyholeCenterX, keyholeCenterY, keyholeRadius } =
    geometry;
  const values = [
    earthCenterX,
    earthCenterY,
    earthRadius,
    keyholeCenterX,
    keyholeCenterY,
    keyholeRadius,
  ];
  if (!values.every(Number.isFinite) || (earthRadius ?? 0) <= 0 || (keyholeRadius ?? 0) <= 0) {
    return false;
  }
  const offset = Math.hypot(earthCenterX - keyholeCenterX, earthCenterY - keyholeCenterY);
  const clearance = keyholeRadius - (offset + earthRadius);
  return clearance >= (wasVisible ? GLOBE_EXIT_CLEARANCE_PX : GLOBE_ENTER_CLEARANCE_PX);
}

/** WGS84 maximum radius in metres (upstream reads Cesium.Ellipsoid for this). */
const EARTH_MAX_RADIUS_M = 6_378_137;

/**
 * The projected Earth-disc radius (px) from camera geometry — upstream's
 * exact angular-size derivation: `asin(R/d)` half-angle scaled by the
 * viewport through the vertical FOV. Null when the camera is inside or at
 * the surface, or the geometry is degenerate.
 */
export function earthDiscScreenRadius(
  cameraDistanceM: number,
  viewportHeightPx: number,
  fovyRad: number,
): number | null {
  if (
    !Number.isFinite(cameraDistanceM) ||
    cameraDistanceM <= EARTH_MAX_RADIUS_M ||
    !(viewportHeightPx > 0) ||
    !Number.isFinite(fovyRad) ||
    fovyRad <= 0 ||
    fovyRad >= Math.PI
  ) {
    return null;
  }
  const angularRadius = Math.asin(Math.min(1, EARTH_MAX_RADIUS_M / cameraDistanceM));
  const radius = (viewportHeightPx * 0.5 * Math.tan(angularRadius)) / Math.tan(fovyRad * 0.5);
  return Number.isFinite(radius) && radius > 0 ? radius : null;
}
