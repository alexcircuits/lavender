import { Cartesian3, Cartographic, Math as CesiumMath, Ellipsoid } from '@cesium/core';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { EntityRef } from './index';
import {
  deadReckon,
  EntityTrackingService,
  LOST_THRESHOLD_MS,
  STALE_THRESHOLD_MS,
  type TrackingCamera,
  type TrackingState,
} from './tracking';

function createMockCamera(initialPosition = new Cartesian3(0, 0, 1000)): TrackingCamera & {
  setViews: Array<{ destination: Cartesian3 }>;
} {
  const setViews: Array<{ destination: Cartesian3 }> = [];
  return {
    position: Cartesian3.clone(initialPosition),
    setViews,
    setView(options) {
      setViews.push(options);
      this.position = Cartesian3.clone(options.destination);
    },
  };
}

describe('EntityTrackingService', () => {
  const target: EntityRef = { feed: 'aircraft', id: 'hex-1234' };
  const origin = Cartesian3.fromDegrees(0, 0, 10_000);

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('tracking -> stale -> lost -> released transitions', () => {
    it('transitions through tracking, stale, lost, and released based on update timestamps', () => {
      const service = new EntityTrackingService();
      const states: TrackingState[] = [];
      service.onStateChanged((s) => states.push(s));

      // 1. Start tracking: status is 'tracking'
      service.startTracking(target);
      expect(service.isTracking()).toBe(true);
      expect(service.getState().status).toBe('tracking');
      expect(service.getState().target).toEqual(target);

      // 2. Initial position update at t = 1000ms
      const baseTime = 1_000;
      service.updateTargetPosition(origin, 90, 450, baseTime);
      expect(service.getState().status).toBe('tracking');
      expect(service.getState().position).toEqual(origin);
      expect(service.getState().headingDeg).toBe(90);
      expect(service.getState().speedKnots).toBe(450);

      // 3. Tick within fresh window (< 30s)
      service.tick(baseTime + 10_000);
      expect(service.getState().status).toBe('tracking');

      // 4. Tick beyond stale threshold (> 30s) -> 'stale'
      service.tick(baseTime + STALE_THRESHOLD_MS + 1_000);
      expect(service.getState().status).toBe('stale');

      // 5. Tick beyond lost threshold (> 60s) -> 'lost'
      service.tick(baseTime + LOST_THRESHOLD_MS + 1_000);
      expect(service.getState().status).toBe('lost');

      // 6. Recovery: new position update recovers back to 'tracking'
      const recoveredPos = Cartesian3.fromDegrees(1, 0, 10_000);
      service.updateTargetPosition(recoveredPos, 90, 450, baseTime + 70_000);
      expect(service.getState().status).toBe('tracking');
      expect(service.getState().position).toEqual(recoveredPos);

      // 7. Calling release transitions to 'released'
      service.release('manual');
      expect(service.isTracking()).toBe(false);
      expect(service.getState().status).toBe('released');
      expect(service.getState().reason).toBe('manual');

      // Further ticks do not resurrect released state
      service.tick(baseTime + 100_000);
      expect(service.getState().status).toBe('released');

      service.destroy();
    });
  });

  describe('user camera interaction interrupts following', () => {
    it('releases tracking on camera interaction', () => {
      const service = new EntityTrackingService();
      service.startTracking(target);
      expect(service.isTracking()).toBe(true);

      // User camera interaction cancels tracking
      service.release('camera_interaction');
      expect(service.isTracking()).toBe(false);
      expect(service.getState().status).toBe('released');
      expect(service.getState().reason).toBe('camera_interaction');

      service.destroy();
    });

    it('cancels tracking when Esc key is pressed', () => {
      const service = new EntityTrackingService();
      service.startTracking(target);
      expect(service.isTracking()).toBe(true);

      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));

      expect(service.isTracking()).toBe(false);
      expect(service.getState().status).toBe('released');
      expect(service.getState().reason).toBe('escape');

      service.destroy();
    });
  });

  describe('dead reckoning projection', () => {
    it('projects position forward using heading and speed', () => {
      // Start at 0, 0 with altitude 0
      const startPos = Cartesian3.fromDegrees(0, 0, 0);
      const headingEast = 90;
      const speedKnots = 600; // ~308.67 m/s
      const elapsedSec = 40; // in 40s travels ~12.35 km

      const projected = deadReckon(startPos, headingEast, speedKnots, elapsedSec);
      const carto = Cartographic.fromCartesian(projected, Ellipsoid.WGS84);
      expect(carto).toBeDefined();
      if (!carto) throw new Error('Carto undefined');

      // Latitude should stay essentially 0; longitude should increase (East)
      expect(CesiumMath.toDegrees(carto.latitude)).toBeCloseTo(0, 4);
      expect(CesiumMath.toDegrees(carto.longitude)).toBeGreaterThan(0);
      const distance = Cartesian3.distance(startPos, projected);
      // Speed 600 kn = 308.667 m/s * 40 s = 12346.67 m
      expect(distance).toBeCloseTo(12_346.67, -1);
    });

    it('dead-reckons when status is stale during tick', () => {
      const service = new EntityTrackingService();
      service.startTracking(target);

      const baseTime = 10_000;
      service.updateTargetPosition(origin, 90, 600, baseTime);

      // When ticking at 45s (> 30s, so stale), the getState().position should be projected
      service.tick(baseTime + 45_000);
      expect(service.getState().status).toBe('stale');

      const projected = service.getState().position;
      expect(projected).not.toBeNull();
      if (!projected) throw new Error('Projected is null');

      // Projected position should be further along heading than origin
      const dist = Cartesian3.distance(origin, projected);
      expect(dist).toBeGreaterThan(1_000);

      service.destroy();
    });
  });

  describe('reduced motion preference', () => {
    it('snaps camera in a single tick when prefersReducedMotion is true', () => {
      const camera = createMockCamera(new Cartesian3(0, 0, 0));
      const targetPos = Cartesian3.fromDegrees(10, 20, 100);

      const service = new EntityTrackingService({
        camera,
        prefersReducedMotion: () => true,
      });

      service.startTracking(target);
      service.updateTargetPosition(targetPos, 0, 0, 1000);
      service.tick(1000);

      expect(camera.setViews.length).toBeGreaterThan(0);
      // With reduced motion, destination was cut/snapped immediately
      const lastView = camera.setViews.at(-1);
      expect(lastView).toBeDefined();
      // Camera position matches destination in 1 tick
      expect(camera.position).toEqual(lastView?.destination);

      service.destroy();
    });

    it('smoothly interpolates camera when prefersReducedMotion is false', () => {
      const camera = createMockCamera(new Cartesian3(0, 0, 0));
      const targetPos = Cartesian3.fromDegrees(10, 20, 100);

      const service = new EntityTrackingService({
        camera,
        prefersReducedMotion: () => false,
      });

      service.startTracking(target);
      service.updateTargetPosition(targetPos, 0, 0, 1000);
      service.tick(1000);

      expect(camera.setViews.length).toBeGreaterThan(0);
      const firstView = camera.setViews.at(-1);
      expect(firstView).toBeDefined();
      if (!firstView) throw new Error('firstView is undefined');

      // In smooth mode, first tick moves partially towards destination (10% step)
      const dist = Cartesian3.distance(new Cartesian3(0, 0, 0), camera.position);
      expect(dist).toBeGreaterThan(0);

      // Compare with reduced motion where camera reaches destination on the first tick:
      const snapCamera = createMockCamera(new Cartesian3(0, 0, 0));
      const snapService = new EntityTrackingService({
        camera: snapCamera,
        prefersReducedMotion: () => true,
      });
      snapService.startTracking(target);
      snapService.updateTargetPosition(targetPos, 0, 0, 1000);
      snapService.tick(1000);

      const fullDist = Cartesian3.distance(new Cartesian3(0, 0, 0), snapCamera.position);
      expect(dist).toBeLessThan(fullDist);
      expect(dist / fullDist).toBeCloseTo(0.1, 2);

      snapService.destroy();
      service.destroy();
    });
  });
});
