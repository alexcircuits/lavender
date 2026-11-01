import { type Cartesian3, Cartographic } from '@cesium/core';
import { describe, expect, it, vi } from 'vitest';
import {
  cancelCameraArrival,
  groundClearanceDeficitM,
  guardCameraAboveGround,
  MIN_EYE_CLEARANCE_M,
  ownCameraArrival,
  sampleSurfaceM,
} from './cameraGroundGuard';

describe('groundClearanceDeficitM pure arithmetic', () => {
  it('lifts a buried camera clear of the surface', () => {
    // Framed near sea level, arriving over ground at ~1,609 m (Denver).
    const lift = groundClearanceDeficitM(140, 1609);
    expect(lift).toBe(1609 + MIN_EYE_CLEARANCE_M - 140);
  });

  it('lifts a camera resting on the surface to a usable clearance', () => {
    // Field report: eye 1 m above mesh at 171 m.
    expect(groundClearanceDeficitM(172, 171)).toBe(MIN_EYE_CLEARANCE_M - 1);
  });

  it('leaves a well-framed arrival untouched', () => {
    expect(groundClearanceDeficitM(1755, 1609)).toBe(0);
    expect(groundClearanceDeficitM(2960, 1609)).toBe(0);
  });

  it('ignores sub-metre noise rather than nudging forever', () => {
    expect(groundClearanceDeficitM(1609 + MIN_EYE_CLEARANCE_M, 1609)).toBe(0);
    expect(groundClearanceDeficitM(1609 + MIN_EYE_CLEARANCE_M - 1, 1609)).toBe(0);
  });

  it('never acts on unmeasurable surface values', () => {
    expect(groundClearanceDeficitM(1000, Number.NaN)).toBe(0);
    expect(groundClearanceDeficitM(Number.NaN, 1609)).toBe(0);
    expect(groundClearanceDeficitM(1000, undefined as unknown as number)).toBe(0);
  });

  it('maintains clearance between 60m and 250m for good framing', () => {
    expect(MIN_EYE_CLEARANCE_M).toBeGreaterThanOrEqual(60);
    expect(MIN_EYE_CLEARANCE_M).toBeLessThanOrEqual(250);
  });
});

describe('sampleSurfaceM', () => {
  it('returns NaN and sampled 0 when scene lacks sampleHeight', () => {
    const result = sampleSurfaceM(null, [{ lat: 30, lon: -97 }]);
    expect(Number.isNaN(result.heightM)).toBe(true);
    expect(result.sampled).toBe(0);
  });

  it('returns the maximum height among sampled cells', () => {
    const scene = {
      sampleHeight: (carto: Cartographic) => {
        if (carto.longitude > 0) return 350;
        return 120;
      },
    };
    const result = sampleSurfaceM(scene, [
      { lat: 10, lon: -20 },
      { lat: 10, lon: 20 },
    ]);
    expect(result.heightM).toBe(350);
    expect(result.sampled).toBe(2);
  });

  it('skips cells where sampleHeight throws or returns non-finite', () => {
    let call = 0;
    const scene = {
      sampleHeight: () => {
        call += 1;
        if (call === 1) throw new Error('tiles loading');
        if (call === 2) return Number.NaN;
        return 500;
      },
    };
    const result = sampleSurfaceM(scene, [
      { lat: 10, lon: 10 },
      { lat: 20, lon: 20 },
      { lat: 30, lon: 30 },
    ]);
    expect(result.heightM).toBe(500);
    expect(result.sampled).toBe(1);
  });
});

interface FakeViewerOptions {
  readonly cameraHeightM: number;
  readonly surfaceM: number | ((carto: Cartographic) => number | undefined);
}

function createFakeViewer({ cameraHeightM, surfaceM }: FakeViewerOptions) {
  const flights: unknown[] = [];
  const position = Cartographic.fromDegrees(-97.7603, 30.3141, cameraHeightM);
  const canvas = new EventTarget();
  const documentRef = new EventTarget();
  Object.defineProperty(canvas, 'ownerDocument', { value: documentRef });

  const camera: Record<string, unknown> = {
    positionCartographic: position,
    heading: 0.5,
    pitch: -0.55,
    roll: 0,
    flyTo: vi.fn((options: unknown) => flights.push(options)),
    flyToBoundingSphere: vi.fn(),
    setView: vi.fn(),
    lookAt: vi.fn(),
    cancelFlight: vi.fn(),
  };

  const trackedEntityListeners = new Set<() => void>();

  const viewer = {
    flights,
    canvas,
    documentRef,
    scene: {
      canvas,
      sampleHeight: (carto: Cartographic) =>
        typeof surfaceM === 'function' ? surfaceM(carto) : surfaceM,
      isDestroyed: () => false,
    },
    camera,
    isDestroyed: () => false,
    trackedEntityChanged: {
      addEventListener: (cb: () => void) => {
        trackedEntityListeners.add(cb);
        return () => trackedEntityListeners.delete(cb);
      },
      raiseEvent: () => {
        for (const cb of trackedEntityListeners) cb();
      },
      numberOfListeners: () => trackedEntityListeners.size,
    },
  };
  return viewer;
}

describe('guardCameraAboveGround lifecycle & corrections', () => {
  it('returns a no-op stop when viewer or coordinates are invalid', () => {
    const stop = guardCameraAboveGround(null, { lat: Number.NaN, lon: 0 });
    expect(typeof stop).toBe('function');
    stop();
  });

  it('lifts a buried arrival once the surface answers', async () => {
    const viewer = createFakeViewer({ cameraHeightM: 172, surfaceM: 171 });
    const lifts: number[] = [];
    guardCameraAboveGround(
      viewer,
      { lat: 30.3125, lon: -97.765 },
      { intervalMs: 1, onLift: (m) => lifts.push(m) },
    );
    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(viewer.flights.length).toBe(1);
    const flight = viewer.flights[0] as {
      destination: Cartesian3;
      orientation: { heading: number; pitch: number; roll: number };
    };
    const lifted = Cartographic.fromCartesian(flight.destination);
    expect(Math.abs(lifted.height - (171 + MIN_EYE_CLEARANCE_M))).toBeLessThan(0.5);
    expect(flight.orientation.heading).toBe(0.5);
    expect(lifts.length).toBe(1);
    expect(lifts[0]).toBe(171 + MIN_EYE_CLEARANCE_M - 172);
  });

  it('waits for streaming tiles and yields to a newer arrival', async () => {
    let answer = Number.NaN;
    let stale = false;
    const viewer = createFakeViewer({ cameraHeightM: 172, surfaceM: () => answer });
    guardCameraAboveGround(
      viewer,
      { lat: 30.3125, lon: -97.765 },
      { intervalMs: 2, attempts: 20, isStale: () => stale },
    );
    await new Promise((resolve) => setTimeout(resolve, 15));
    expect(viewer.flights.length).toBe(0);
    stale = true;
    answer = 171;
    await new Promise((resolve) => setTimeout(resolve, 15));
    expect(viewer.flights.length).toBe(0);
  });

  it('leaves a clear view untouched', async () => {
    const viewer = createFakeViewer({ cameraHeightM: 400, surfaceM: 171 });
    guardCameraAboveGround(viewer, { lat: 30.3125, lon: -97.765 }, { intervalMs: 1 });
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(viewer.flights.length).toBe(0);
  });

  it('stops quietly when attempts are exhausted without measuring surface', async () => {
    const viewer = createFakeViewer({ cameraHeightM: 172, surfaceM: () => Number.NaN });
    guardCameraAboveGround(viewer, { lat: 30.3125, lon: -97.765 }, { intervalMs: 1, attempts: 2 });
    await new Promise((resolve) => setTimeout(resolve, 25));
    expect(viewer.flights.length).toBe(0);
  });

  it('relinquishes guard on user pointerdown gesture', async () => {
    const viewer = createFakeViewer({ cameraHeightM: 172, surfaceM: () => Number.NaN });
    guardCameraAboveGround(viewer, { lat: 30.3125, lon: -97.765 }, { intervalMs: 5 });
    viewer.canvas.dispatchEvent(new Event('pointerdown'));
    viewer.scene.sampleHeight = () => 171;
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(viewer.flights.length).toBe(0);
  });

  it('relinquishes guard on user wheel gesture', async () => {
    const viewer = createFakeViewer({ cameraHeightM: 172, surfaceM: () => Number.NaN });
    guardCameraAboveGround(viewer, { lat: 30.3125, lon: -97.765 }, { intervalMs: 5 });
    viewer.canvas.dispatchEvent(new Event('wheel'));
    viewer.scene.sampleHeight = () => 171;
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(viewer.flights.length).toBe(0);
  });

  it('relinquishes guard on keydown', async () => {
    const viewer = createFakeViewer({ cameraHeightM: 172, surfaceM: () => Number.NaN });
    guardCameraAboveGround(viewer, { lat: 30.3125, lon: -97.765 }, { intervalMs: 5 });
    viewer.documentRef.dispatchEvent(new Event('keydown'));
    viewer.scene.sampleHeight = () => 171;
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(viewer.flights.length).toBe(0);
  });

  for (const method of [
    'flyTo',
    'flyToBoundingSphere',
    'setView',
    'lookAt',
    'cancelFlight',
  ] as const) {
    it(`relinquishes guard and restores descriptor when camera.${method}() is called`, async () => {
      const viewer = createFakeViewer({ cameraHeightM: 172, surfaceM: 171 });
      const originalMethod = viewer.camera[method];
      guardCameraAboveGround(viewer, { lat: 30.3125, lon: -97.765 }, { intervalMs: 10 });
      // Calling the direct camera method stops the guard immediately
      (viewer.camera[method] as (...args: unknown[]) => void)();
      const priorFlights = viewer.flights.length;
      expect(viewer.camera[method]).toBe(originalMethod);
      await new Promise((resolve) => setTimeout(resolve, 25));
      expect(viewer.flights.length).toBe(priorFlights);
    });
  }

  it('relinquishes guard on trackedEntityChanged', async () => {
    const viewer = createFakeViewer({ cameraHeightM: 172, surfaceM: 171 });
    guardCameraAboveGround(viewer, { lat: 30.3125, lon: -97.765 }, { intervalMs: 10 });
    viewer.trackedEntityChanged.raiseEvent();
    expect(viewer.trackedEntityChanged.numberOfListeners()).toBe(0);
    await new Promise((resolve) => setTimeout(resolve, 25));
    expect(viewer.flights.length).toBe(0);
  });

  it('cancel function stops the guard immediately', async () => {
    const viewer = createFakeViewer({ cameraHeightM: 172, surfaceM: 171 });
    const cancel = guardCameraAboveGround(
      viewer,
      { lat: 30.3125, lon: -97.765 },
      { intervalMs: 10 },
    );
    cancel();
    await new Promise((resolve) => setTimeout(resolve, 25));
    expect(viewer.flights.length).toBe(0);
  });

  it('cancels previous arrival guard when a new one starts on the same viewer', async () => {
    const viewer = createFakeViewer({ cameraHeightM: 172, surfaceM: 171 });
    const lifts: number[] = [];
    guardCameraAboveGround(
      viewer,
      { lat: 30, lon: -97 },
      { intervalMs: 15, onLift: () => lifts.push(1) },
    );
    guardCameraAboveGround(
      viewer,
      { lat: 40, lon: -105 },
      { intervalMs: 1, onLift: () => lifts.push(2) },
    );
    await new Promise((resolve) => setTimeout(resolve, 25));
    expect(lifts).toEqual([2]);
  });

  it('cancelCameraArrival cancels any active arrival registered via ownCameraArrival', () => {
    const fakeViewerObj = {};
    let cancelled = false;
    ownCameraArrival(fakeViewerObj, () => {
      cancelled = true;
    });
    cancelCameraArrival(fakeViewerObj);
    expect(cancelled).toBe(true);
  });
});
