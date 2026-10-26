/**
 * Entity tracking and camera follow state machine (ENG-977).
 *
 * States:
 * - 'tracking': actively following live reports (< 30s old).
 * - 'stale': no report for > 30s; dead-reckons forward using known velocity and heading.
 * - 'lost': no report for > 60s; dead reckoning stops and target is marked lost.
 * - 'released': following cancelled by user interaction, explicit release, or Esc key.
 */
import { Cartesian3, Cartographic, Math as CesiumMath, Ellipsoid } from '@cesium/core';
import { prefersReducedMotion as defaultPrefersReducedMotion } from './camera';
import type { EntityRef } from './index';
import { Listeners } from './listeners';
import { slewHeading } from './renderers/wasmSkyKinematics';

export type TrackingStatus = 'tracking' | 'stale' | 'lost' | 'released';
export type TrackingMode = 'follow' | 'cockpit';

export interface TrackingState {
  readonly status: TrackingStatus;
  readonly mode: TrackingMode;
  readonly target: EntityRef | null;
  readonly position: Cartesian3 | null;
  readonly headingDeg?: number | null;
  readonly speedKnots?: number | null;
  readonly lastUpdateMs: number | null;
  readonly reason?: string | null;
}

export interface TrackingCamera {
  position: Cartesian3;
  readonly positionCartographic?: Cartographic;
  setView(options: {
    destination: Cartesian3;
    orientation?: { heading?: number; pitch?: number; roll?: number };
  }): void;
}

export interface TrackingOptions {
  readonly camera?: TrackingCamera | null;
  readonly prefersReducedMotion?: () => boolean;
  readonly requestRender?: () => void;
}

export const STALE_THRESHOLD_MS = 30_000;
export const LOST_THRESHOLD_MS = 60_000;

/** Knots to metres per second: 1 knot = 1852 m / 3600 s. */
const KNOTS_TO_MPS = 1852 / 3600;

/**
 * Projects a target position forward along a great-circle path given heading and speed.
 */
export function deadReckon(
  position: Cartesian3,
  headingDeg: number,
  speedKnots: number,
  elapsedSec: number,
): Cartesian3 {
  if (elapsedSec <= 0 || speedKnots <= 0 || !Number.isFinite(speedKnots)) {
    return Cartesian3.clone(position);
  }
  const carto = Cartographic.fromCartesian(position, Ellipsoid.WGS84);
  if (!carto) return Cartesian3.clone(position);

  const lat1 = carto.latitude;
  const lon1 = carto.longitude;
  const heightM = carto.height;

  const distanceM = speedKnots * KNOTS_TO_MPS * elapsedSec;
  const angularDist = distanceM / Ellipsoid.WGS84.maximumRadius;
  const headingRad = CesiumMath.toRadians(headingDeg);

  const sinLat1 = Math.sin(lat1);
  const cosLat1 = Math.cos(lat1);
  const sinAng = Math.sin(angularDist);
  const cosAng = Math.cos(angularDist);

  const sinLat2 = Math.min(
    1,
    Math.max(-1, sinLat1 * cosAng + cosLat1 * sinAng * Math.cos(headingRad)),
  );
  const lat2 = Math.asin(sinLat2);
  const y = Math.sin(headingRad) * sinAng * cosLat1;
  const x = cosAng - sinLat1 * sinLat2;
  const lon2 = lon1 + Math.atan2(y, x);

  return Cartesian3.fromRadians(lon2, lat2, heightM, Ellipsoid.WGS84);
}

export class EntityTrackingService {
  readonly #camera: TrackingCamera | null;
  readonly #prefersReducedMotion: () => boolean;
  readonly #requestRender: (() => void) | undefined;
  readonly #listeners = new Listeners<TrackingState>('tracking state');

  #status: TrackingStatus = 'released';
  #mode: TrackingMode = 'follow';
  #target: EntityRef | null = null;
  #position: Cartesian3 | null = null;
  #headingDeg: number | null = null;
  #speedKnots: number | null = null;
  #lastUpdateMs: number | null = null;
  #reason: string | null = null;
  #lastProjected: Cartesian3 | null = null;
  #currentSlewHeadingDeg: number | null = null;
  #keyDownHandler: ((event: KeyboardEvent) => void) | null = null;

  constructor(options: TrackingOptions = {}) {
    this.#camera = options.camera ?? null;
    this.#prefersReducedMotion = options.prefersReducedMotion ?? defaultPrefersReducedMotion;
    this.#requestRender = options.requestRender;

    if (typeof window !== 'undefined' && typeof window.addEventListener === 'function') {
      this.#keyDownHandler = (event: KeyboardEvent) => {
        if (event.key === 'Escape' && this.isTracking()) {
          this.release('escape');
        }
      };
      window.addEventListener('keydown', this.#keyDownHandler);
    }
  }

  isTracking(): boolean {
    return this.#status !== 'released';
  }

  getState(): TrackingState {
    const position =
      this.#status === 'stale' && this.#lastProjected ? this.#lastProjected : this.#position;
    return {
      status: this.#status,
      mode: this.#mode,
      target: this.#target,
      position,
      headingDeg: this.#headingDeg,
      speedKnots: this.#speedKnots,
      lastUpdateMs: this.#lastUpdateMs,
      reason: this.#reason,
    };
  }

  onStateChanged(listener: (state: TrackingState) => void): () => void {
    return this.#listeners.add(listener);
  }

  startTracking(target: EntityRef, mode: TrackingMode = 'follow'): void {
    this.#target = target;
    this.#mode = mode;
    this.#status = 'tracking';
    this.#position = null;
    this.#headingDeg = null;
    this.#speedKnots = null;
    this.#lastUpdateMs = null;
    this.#reason = null;
    this.#lastProjected = null;
    this.#currentSlewHeadingDeg = null;
    this.#emitState();
  }

  setMode(mode: TrackingMode): void {
    if (this.#mode === mode) return;
    this.#mode = mode;
    this.#emitState();
  }

  updateTargetPosition(
    position: Cartesian3 | { lat: number; lon: number; heightM?: number },
    headingDeg?: number,
    speedKnots?: number,
    timestampMs?: number,
  ): void {
    if (this.#status === 'released') return;

    const cartesian =
      'lat' in position && 'lon' in position
        ? Cartesian3.fromDegrees(position.lon, position.lat, position.heightM ?? 0, Ellipsoid.WGS84)
        : Cartesian3.clone(position);

    this.#position = cartesian;
    this.#headingDeg = headingDeg !== undefined && Number.isFinite(headingDeg) ? headingDeg : null;
    this.#speedKnots = speedKnots !== undefined && Number.isFinite(speedKnots) ? speedKnots : null;
    this.#lastUpdateMs = timestampMs ?? Date.now();
    this.#lastProjected = null;

    if (this.#status === 'stale' || this.#status === 'lost') {
      this.#status = 'tracking';
    }

    this.#emitState();
  }

  tick(timeMs: number): void {
    if (this.#status === 'released' || !this.#target) return;

    const previousStatus = this.#status;

    if (this.#lastUpdateMs !== null) {
      const elapsedMs = timeMs - this.#lastUpdateMs;
      if (elapsedMs > LOST_THRESHOLD_MS) {
        this.#status = 'lost';
      } else if (elapsedMs > STALE_THRESHOLD_MS) {
        this.#status = 'stale';
      } else {
        this.#status = 'tracking';
      }
    }

    let targetPos = this.#position;
    if (
      this.#status === 'stale' &&
      this.#position &&
      this.#lastUpdateMs !== null &&
      this.#headingDeg !== null &&
      this.#speedKnots !== null
    ) {
      const elapsedSec = (timeMs - this.#lastUpdateMs) / 1000;
      targetPos = deadReckon(this.#position, this.#headingDeg, this.#speedKnots, elapsedSec);
      this.#lastProjected = targetPos;
    }

    if (this.#status !== previousStatus) {
      this.#emitState();
    }

    if (this.#camera && targetPos) {
      this.#moveCamera(targetPos);
    }
  }

  release(reason = 'manual'): void {
    if (this.#status === 'released') return;
    this.#status = 'released';
    this.#reason = reason;
    this.#emitState();
  }

  destroy(): void {
    if (this.#keyDownHandler && typeof window !== 'undefined') {
      window.removeEventListener('keydown', this.#keyDownHandler);
      this.#keyDownHandler = null;
    }
    this.#listeners.clear();
  }

  #moveCamera(targetPos: Cartesian3): void {
    if (!this.#camera) return;

    if (this.#mode === 'cockpit') {
      this.#moveCameraCockpit(targetPos);
      return;
    }

    let destination = targetPos;
    const carto = Cartographic.fromCartesian(targetPos, Ellipsoid.WGS84);
    if (carto) {
      const currentHeight = this.#camera.positionCartographic?.height ?? 100_000;
      const followHeight = Math.max(1_000, currentHeight);
      destination = Cartesian3.fromRadians(
        carto.longitude,
        carto.latitude,
        carto.height + followHeight,
        Ellipsoid.WGS84,
      );
    }

    const reducedMotion = this.#prefersReducedMotion();
    if (reducedMotion) {
      this.#camera.position = Cartesian3.clone(destination);
      this.#camera.setView({ destination });
    } else {
      const next = Cartesian3.lerp(this.#camera.position, destination, 0.1, new Cartesian3());
      this.#camera.position = next;
      this.#camera.setView({ destination: next });
    }

    this.#requestRender?.();
  }

  #moveCameraCockpit(targetPos: Cartesian3): void {
    if (!this.#camera) return;
    const carto = Cartographic.fromCartesian(targetPos, Ellipsoid.WGS84);
    if (!carto) return;

    const targetHdg = this.#headingDeg ?? 0;
    const curHdg = this.#currentSlewHeadingDeg ?? targetHdg;
    const slewedHdg = slewHeading(curHdg, targetHdg, 2.5);
    this.#currentSlewHeadingDeg = slewedHdg;

    const hdgRad = CesiumMath.toRadians(slewedHdg);
    const pitchRad = CesiumMath.toRadians(-3.0);
    const r = Ellipsoid.WGS84.maximumRadius;

    // Offset camera 15m forward, 3m up
    const dLat = (15 * Math.cos(hdgRad)) / r;
    const dLon = (15 * Math.sin(hdgRad)) / (r * Math.max(0.01, Math.cos(carto.latitude)));
    const cameraHeight = carto.height + 3;

    const destination = Cartesian3.fromRadians(
      carto.longitude + dLon,
      carto.latitude + dLat,
      cameraHeight,
      Ellipsoid.WGS84,
    );

    this.#camera.position = destination;
    this.#camera.setView({
      destination,
      orientation: {
        heading: hdgRad,
        pitch: pitchRad,
        roll: 0,
      },
    });

    this.#requestRender?.();
  }

  #emitState(): void {
    this.#listeners.emit(this.getState());
  }
}

export { EntityTrackingService as TrackingController };
