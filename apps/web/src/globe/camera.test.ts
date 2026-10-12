import { describe, expect, it, vi } from 'vitest';
import {
  type CameraPort,
  CameraService,
  DEFAULT_POINT_HEIGHT_M,
  type Destination,
  destinationFor,
  FLIGHT_DURATION_S,
  HOME_VIEW,
  MAX_HEIGHT_M,
  MIN_HEIGHT_M,
  NORTH_UP_DURATION_S,
  standardEasing,
  zoomAmount,
} from './camera';

interface Recorded {
  flights: Array<{ destination: Destination; durationS: number }>;
  orients: Array<{ headingRad: number; pitchRad: number; durationS: number }>;
  zooms: number[];
}

function fakeCamera(heightM = 5_000_000, pitchRad = -0.7): CameraPort & Recorded {
  const recorded: Recorded = { flights: [], orients: [], zooms: [] };
  return {
    ...recorded,
    heightM: () => heightM,
    pitchRad: () => pitchRad,
    flyTo: (destination, durationS) => recorded.flights.push({ destination, durationS }),
    orient: (headingRad, pitch, durationS) =>
      recorded.orients.push({ headingRad, pitchRad: pitch, durationS }),
    zoom: (amountM) => recorded.zooms.push(amountM),
  };
}

describe('CameraService', () => {
  it('flies in 1.2 s normally', () => {
    const port = fakeCamera();
    new CameraService(port, () => false).flyTo({
      kind: 'point',
      position: { lat: 51.5, lon: -0.12 },
    });
    expect(port.flights).toEqual([
      {
        destination: { kind: 'point', lat: 51.5, lon: -0.12, heightM: DEFAULT_POINT_HEIGHT_M },
        durationS: FLIGHT_DURATION_S,
      },
    ]);
  });

  it('cuts instantly (duration 0) under prefers-reduced-motion', () => {
    const port = fakeCamera();
    let reduced = true;
    const camera = new CameraService(port, () => reduced);
    camera.flyTo({ kind: 'bbox', bbox: { west: 0, south: 0, east: 10, north: 10 } });
    camera.home();
    camera.resetNorth();
    expect(port.flights.map((f) => f.durationS)).toEqual([0, 0]);
    expect(port.orients.map((o) => o.durationS)).toEqual([0]);
    // Evaluated per command: the preference applies as soon as it changes.
    reduced = false;
    camera.home();
    expect(port.flights.at(-1)?.durationS).toBe(FLIGHT_DURATION_S);
  });

  it('flies home to the whole-earth view', () => {
    const port = fakeCamera();
    new CameraService(port, () => false).home();
    expect(port.flights[0]?.destination).toEqual({ kind: 'point', ...HOME_VIEW });
  });

  it('honours an explicit point height within limits', () => {
    const point = (heightM: number) =>
      destinationFor({ kind: 'point', position: { lat: 0, lon: 0 }, heightM });
    expect(point(2_000)).toMatchObject({ heightM: 2_000 });
    expect(point(1)).toMatchObject({ heightM: MIN_HEIGHT_M });
    expect(point(1e12)).toMatchObject({ heightM: MAX_HEIGHT_M });
  });

  it('ignores targets with unusable coordinates', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const port = fakeCamera();
    const camera = new CameraService(port, () => false);
    camera.flyTo({ kind: 'point', position: { lat: Number.NaN, lon: 0 } });
    camera.flyTo({ kind: 'point', position: { lat: 95, lon: 0 } });
    camera.flyTo({
      kind: 'bbox',
      bbox: { west: 0, south: 0, east: Number.POSITIVE_INFINITY, north: 1 },
    });
    camera.flyTo({ kind: 'bbox', bbox: { west: 0, south: 95, east: 10, north: 100 } });
    expect(port.flights).toEqual([]);
    expect(warn).toHaveBeenCalledTimes(4);
  });

  it('pads boxes, keeps antimeridian crossings and never dives into a point', () => {
    expect(
      destinationFor({ kind: 'bbox', bbox: { west: 0, south: 0, east: 10, north: 10 } }),
    ).toEqual({ kind: 'rectangle', west: -1, south: -1, east: 11, north: 11 });
    const crossing = destinationFor({
      kind: 'bbox',
      bbox: { west: 170, south: -10, east: -170, north: 10 },
    });
    expect(crossing).toMatchObject({ kind: 'rectangle', west: 168, east: -168 });
    const tiny = destinationFor({ kind: 'bbox', bbox: { west: 5, south: 5, east: 5, north: 5 } });
    if (tiny?.kind !== 'rectangle') throw new Error('expected a rectangle');
    expect(tiny.east - tiny.west).toBeCloseTo(0.02, 9);
    expect(tiny.north - tiny.south).toBeCloseTo(0.02, 9);
    const polar = destinationFor({
      kind: 'bbox',
      bbox: { west: -10, south: 80, east: 10, north: 90 },
    });
    expect(polar).toMatchObject({ north: 90 });
  });

  it('zooms in and out in proportion to the height', () => {
    const port = fakeCamera(1_000_000);
    const camera = new CameraService(port, () => false);
    camera.zoomIn();
    camera.zoomOut();
    expect(port.zooms).toEqual([500_000, -1_000_000]);
    expect(zoomAmount(150, 'in')).toBe(50); // never below the minimum height
    expect(Math.abs(zoomAmount(MAX_HEIGHT_M, 'out'))).toBe(0); // never beyond the maximum
  });

  it('turns north-up in place, keeping the pitch', () => {
    const port = fakeCamera(3_000_000, -0.5);
    new CameraService(port, () => false).resetNorth();
    expect(port.orients).toEqual([
      { headingRad: 0, pitchRad: -0.5, durationS: NORTH_UP_DURATION_S },
    ]);
    expect(port.flights).toEqual([]);
  });
});

describe('standard easing', () => {
  it('runs from 0 to 1 monotonically', () => {
    expect(standardEasing(0)).toBe(0);
    expect(standardEasing(1)).toBe(1);
    let previous = 0;
    for (let t = 0.05; t < 1; t += 0.05) {
      const value = standardEasing(t);
      expect(value).toBeGreaterThanOrEqual(previous);
      previous = value;
    }
    // cubic-bezier(.2, 0, 0, 1) decelerates: well past halfway at half time.
    expect(standardEasing(0.5)).toBeGreaterThan(0.75);
  });
});
