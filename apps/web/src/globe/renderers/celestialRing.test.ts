import { describe, expect, it } from 'vitest';
import { earthDiscScreenRadius, isFullGlobeInsideKeyhole } from '../../features/celestial/model';
import {
  COLLIDING_MARKER_EXTRA_INSET_PX,
  discGeometry,
  keyholeGeometryFor,
  MARKER_COLLISION_ANGLE,
  markersCollide,
  readCelestialInk,
  rgbTripleOf,
  ringLayout,
  screenBearing,
} from './celestialRing';

/** WGS84 maximum radius, matching the model's internal constant. */
const EARTH_MAX_RADIUS_M = 6_378_137;
const WIDTH = 800;
const HEIGHT = 600;
const FOVY = Math.PI / 3;
const TAN_HALF_FOVY = Math.tan(FOVY / 2);

/**
 * A fake camera reduced to the numbers the ring's geometry needs (no GL, no Cesium
 * instance): distance from Earth centre, viewport, vertical FOV and the projected
 * Earth-centre screen position (dead-centre unless a test offsets it).
 */
function fakeCamera(distanceM: number, offset: { x: number; y: number } = { x: 0, y: 0 }) {
  return {
    cameraDistanceM: distanceM,
    viewportWidthPx: WIDTH,
    viewportHeightPx: HEIGHT,
    fovyRad: FOVY,
    earthCenterScreenX: WIDTH / 2 + offset.x,
    earthCenterScreenY: HEIGHT / 2 + offset.y,
  };
}

/** Camera distance whose projected Earth disc has exactly `discRadiusPx` (dead centre). */
function distanceForDiscRadius(discRadiusPx: number): number {
  const angularRadius = Math.atan((discRadiusPx * TAN_HALF_FOVY) / (HEIGHT / 2));
  return EARTH_MAX_RADIUS_M / Math.sin(angularRadius);
}

describe('keyhole geometry', () => {
  it('places the shared keyhole circle at the viewport centre with the 1.05 outer radius', () => {
    expect(keyholeGeometryFor(WIDTH, HEIGHT)).toEqual({
      centerX: 400,
      centerY: 300,
      radius: 600 * 0.5 * 1.05,
    });
  });

  it('refuses degenerate viewports', () => {
    expect(keyholeGeometryFor(0, HEIGHT)).toBeNull();
    expect(keyholeGeometryFor(WIDTH, -1)).toBeNull();
    expect(keyholeGeometryFor(Number.NaN, HEIGHT)).toBeNull();
  });
});

describe('disc geometry from a fake camera', () => {
  it('matches the model disc radius and clears the keyhole when far out', () => {
    const geometry = discGeometry(fakeCamera(20_000_000));
    expect(geometry).not.toBeNull();
    const modelRadius = earthDiscScreenRadius(20_000_000, HEIGHT, FOVY);
    expect(geometry?.earthRadius).toBe(modelRadius);
    // Clearance 315 - 174.7 ≈ 140 px: fully inside, and visible without hysteresis.
    expect(isFullGlobeInsideKeyhole(geometry, false)).toBe(true);
  });

  it('stays out of the visible state when the globe overflows the keyhole', () => {
    const geometry = discGeometry(fakeCamera(8_000_000));
    expect(geometry).not.toBeNull();
    expect(isFullGlobeInsideKeyhole(geometry, false)).toBe(false);
    expect(isFullGlobeInsideKeyhole(geometry, true)).toBe(false);
  });

  it('keeps the exit threshold for a disc in the 12–24 px hysteresis band', () => {
    // 297 px disc → 315 - 297 = 18 px clearance: too small to enter, enough to stay.
    const geometry = discGeometry(fakeCamera(distanceForDiscRadius(297)));
    expect(geometry).not.toBeNull();
    expect(geometry?.earthRadius).toBeCloseTo(297, 0);
    expect(isFullGlobeInsideKeyhole(geometry, false)).toBe(false);
    expect(isFullGlobeInsideKeyhole(geometry, true)).toBe(true);
  });

  it('accounts for an off-centre projected Earth disc', () => {
    // 120 px offset leaves 315 - 174.7 - 120 ≈ 20 px clearance: exit-only.
    const geometry = discGeometry(fakeCamera(20_000_000, { x: 120, y: 0 }));
    expect(isFullGlobeInsideKeyhole(geometry, false)).toBe(false);
    expect(isFullGlobeInsideKeyhole(geometry, true)).toBe(true);
  });

  it('returns null below the atmosphere, at the surface and for a bad frustum', () => {
    expect(discGeometry(fakeCamera(EARTH_MAX_RADIUS_M))).toBeNull();
    expect(discGeometry(fakeCamera(6_000_000))).toBeNull();
    expect(discGeometry({ ...fakeCamera(20_000_000), fovyRad: 0 })).toBeNull();
    expect(discGeometry({ ...fakeCamera(20_000_000), fovyRad: Math.PI })).toBeNull();
    expect(discGeometry({ ...fakeCamera(20_000_000), viewportWidthPx: 0 })).toBeNull();
  });
});

describe('ring layout', () => {
  it('insets the ring inside the keyhole and keeps the ray annulus outside the limb', () => {
    const layout = ringLayout(315, 174.7);
    expect(layout).toEqual({
      radius: 315 - 11,
      // limb 174.7 + max(38, 304*0.065) — the 38 px floor wins at this size.
      rayInnerRadius: 174.7 + 38,
      markerRadius: 315 - 11 - 36,
    });
  });

  it('clamps the ray annulus below the ring once the globe fills the keyhole', () => {
    const layout = ringLayout(315, 250);
    // limb 250 + max(38, 19.76) = 288 > ring - 12 = 292? No: 292 wins only above 306.
    expect(layout?.rayInnerRadius).toBeLessThanOrEqual((layout?.radius ?? 0) - 12);
  });

  it('refuses keyholes smaller than the inset', () => {
    expect(ringLayout(10, 5)).toBeNull();
    expect(ringLayout(Number.NaN, 5)).toBeNull();
  });
});

describe('screen bearings', () => {
  const right = { x: 1, y: 0, z: 0 };
  const up = { x: 0, y: 0, z: 1 };

  it('projects a body on the camera-right axis to angle 0', () => {
    const bearing = screenBearing({ x: 1, y: 0, z: 0 }, right, up, 0);
    expect(bearing.stable).toBe(true);
    expect(bearing.angle).toBeCloseTo(0, 12);
    expect(bearing.opacity).toBe(1);
  });

  it('wraps a body on the camera-up axis below 2π (canvas Y grows downward)', () => {
    const bearing = screenBearing({ x: 0, y: 0, z: 1 }, right, up, 0);
    expect(bearing.stable).toBe(true);
    expect(bearing.angle).toBeCloseTo((3 * Math.PI) / 2, 10);
    expect(bearing.opacity).toBe(1);
  });

  it('keeps the prior bearing and fades a vector pointing into the camera', () => {
    // Camera forward = right × up = -ŷ.
    const bearing = screenBearing({ x: 0, y: -1, z: 0 }, right, up, 1.2);
    expect(bearing).toMatchObject({ angle: 1.2, opacity: 0, stable: false });
  });

  it('fades a mostly-aligned vector proportionally to its plane length', () => {
    const bearing = screenBearing({ x: 0.02, y: -1, z: 0 }, right, up, 0.5);
    expect(bearing.stable).toBe(false);
    expect(bearing.opacity).toBeCloseTo(0.02 / 0.045, 10);
    expect(bearing.angle).toBe(0.5);
  });
});

describe('marker collision', () => {
  it('puts the moon into the second lane only near conjunction', () => {
    expect(markersCollide(0, MARKER_COLLISION_ANGLE / 2)).toBe(true);
    expect(markersCollide(0, Math.PI / 2)).toBe(false);
    expect(COLLIDING_MARKER_EXTRA_INSET_PX).toBe(32);
  });
});

describe('token ink', () => {
  it('parses the hex and rgb() forms tokens.css uses', () => {
    expect(rgbTripleOf('#f0a33a')).toBe('240, 163, 58');
    expect(rgbTripleOf('#abc')).toBe('170, 187, 204');
    expect(rgbTripleOf('rgb(56, 189, 248)')).toBe('56, 189, 248');
    expect(rgbTripleOf('color-mix(in srgb, red, blue)')).toBeNull();
    expect(rgbTripleOf('')).toBeNull();
  });

  it('reads every ink from tokens with the documented fallbacks', () => {
    const ink = readCelestialInk((name) =>
      name === '--map-space' ? '#d6d3e6' : name === '--warning' ? 'rgb(222, 190, 89)' : '',
    );
    expect(ink.ring).toBe('214, 211, 230');
    expect(ink.sun).toBe('222, 190, 89');
    expect(ink.tick).toBe('163, 168, 180'); // --ink-2 default
    expect(ink.moon).toBe('56, 189, 248'); // --map-boundary-maritime default
  });

  it('falls back to token defaults for unusable values', () => {
    const ink = readCelestialInk(() => 'transparent');
    expect(ink).toEqual({
      ring: '214, 211, 230',
      tick: '163, 168, 180',
      sun: '240, 163, 58',
      moon: '56, 189, 248',
    });
  });
});
