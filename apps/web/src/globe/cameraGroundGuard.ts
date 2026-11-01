/**
 * Camera ground guard: never leave an arrival buried in, or pressed against,
 * the rendered surface.
 *
 * Arrival framing is computed before the destination's tiles exist, so it has
 * to predict the ground height. `globe.getHeight()` answers nothing while the
 * globe is hidden under photoreal tiles, curated `groundElevation` presets
 * cover only a few cities, and the elevation service resolves asynchronously.
 * A missing prediction used to fall back to sea level, which buried the eye by
 * roughly the local elevation (Denver ~1,600 m; Austin ~170 m).
 *
 * After the flight lands and tiles have streamed in, this module measures the
 * rendered surface under the framed target and under the camera itself, and
 * lifts the eye if it sits below a usable clearance. Heading and pitch are
 * held, so the correction reads as the shot settling.
 */
import { Cartesian3, Cartographic, Math as CesiumMath } from '@cesium/core';
import { EasingFunction } from '@cesium/engine';

/** Minimum eye height above the rendered surface for a usable view. */
export const MIN_EYE_CLEARANCE_M = 120;
/** Retries while tiles stream in; sampling fails until the mesh exists. */
const GUARD_ATTEMPTS = 8;
const GUARD_INTERVAL_MS = 600;
/** Ignore sub-metre noise rather than nudging the camera forever. */
const GUARD_EPSILON_M = 2;

const probeScratch = new Cartographic();

/** Arrival corrections belong to one viewer and yield to its next camera owner. */
const arrivals = new WeakMap<object, () => void>();

/** Cancel an outstanding arrival correction before navigation or teardown. */
export function cancelCameraArrival(viewer: unknown): void {
  if (viewer && (typeof viewer === 'object' || typeof viewer === 'function')) {
    arrivals.get(viewer)?.();
  }
}

/** Register a correction and return an idempotent ownership release. */
export function ownCameraArrival(viewer: object, cancel: () => void): () => void {
  cancelCameraArrival(viewer);
  arrivals.set(viewer, cancel);
  return () => {
    if (arrivals.get(viewer) === cancel) {
      arrivals.delete(viewer);
    }
  };
}

export interface GroundProbeCell {
  readonly lat: number;
  readonly lon: number;
}

export interface SurfaceSampleResult {
  readonly heightM: number;
  readonly sampled: number;
}

/**
 * Highest rendered surface across a few cells, with how many cells answered.
 * A cell whose tiles are not streamed yet contributes nothing.
 */
export function sampleSurfaceM(
  scene: unknown,
  cells: readonly GroundProbeCell[],
): SurfaceSampleResult {
  let heightM = Number.NaN;
  let sampled = 0;
  const s = scene as
    | { sampleHeight?: (position: Cartographic) => number | undefined }
    | null
    | undefined;
  if (typeof s?.sampleHeight !== 'function') return { heightM, sampled };
  for (const cell of cells) {
    try {
      const height = s.sampleHeight(Cartographic.fromDegrees(cell.lon, cell.lat, 0, probeScratch));
      if (!Number.isFinite(height)) continue;
      sampled += 1;
      heightM = Number.isFinite(heightM) ? Math.max(heightM, height as number) : (height as number);
    } catch {
      /* tiles not ready for this cell */
    }
  }
  return { heightM, sampled };
}

/**
 * How far the camera must rise to clear the ground, given a measured surface.
 * Pure so the policy is testable without a scene.
 */
export function groundClearanceDeficitM(
  cameraHeightM: number,
  groundHeightM: number,
  clearanceM: number = MIN_EYE_CLEARANCE_M,
): number {
  if (!Number.isFinite(cameraHeightM) || !Number.isFinite(groundHeightM)) {
    return 0;
  }
  const deficit = groundHeightM + clearanceM - cameraHeightM;
  return deficit > GUARD_EPSILON_M ? deficit : 0;
}

export interface GuardCameraOptions {
  readonly clearanceM?: number;
  readonly attempts?: number;
  readonly intervalMs?: number;
  readonly isStale?: () => boolean;
  readonly onLift?: (liftM: number) => void;
}

export interface GuardCameraFlightOptions {
  readonly destination: Cartesian3;
  readonly orientation: {
    readonly heading: number;
    readonly pitch: number;
    readonly roll: number;
  };
  readonly duration: number;
  readonly easingFunction: (time: number) => number;
}

export interface GuardViewerCamera {
  readonly positionCartographic?:
    | Cartographic
    | { readonly longitude: number; readonly latitude: number; readonly height: number };
  readonly heading?: number;
  readonly pitch?: number;
  readonly roll?: number;
  flyTo?: (options: GuardCameraFlightOptions) => void;
  flyToBoundingSphere?: (...args: unknown[]) => unknown;
  setView?: (...args: unknown[]) => unknown;
  lookAt?: (...args: unknown[]) => unknown;
  cancelFlight?: (...args: unknown[]) => unknown;
  [key: string]: unknown;
}

export interface GuardViewerScene {
  readonly canvas?: (EventTarget & { readonly ownerDocument?: Document | null }) | null;
  readonly camera?: GuardViewerCamera | null;
  sampleHeight?: (cartographic: Cartographic) => number | undefined;
  isDestroyed?: () => boolean;
}

export interface GuardViewer {
  readonly scene?: GuardViewerScene | null;
  readonly camera?: GuardViewerCamera | null;
  isDestroyed?: () => boolean;
  trackedEntityChanged?: {
    addEventListener?: (listener: () => void) => (() => void) | undefined;
  };
}

/**
 * Watch an arrival and lift the camera once the real surface can be measured.
 *
 * Stops quietly on a clear view, exhausted attempts, the user taking the
 * controls (pointer or wheel on the canvas), or `isStale()` reporting that a
 * newer arrival owns the camera.
 */
export function guardCameraAboveGround(
  viewer: unknown,
  at: GroundProbeCell,
  options: GuardCameraOptions = {},
): () => void {
  const {
    clearanceM = MIN_EYE_CLEARANCE_M,
    attempts = GUARD_ATTEMPTS,
    intervalMs = GUARD_INTERVAL_MS,
    isStale = () => false,
    onLift = null,
  } = options;

  if (
    !viewer ||
    typeof viewer !== 'object' ||
    !('scene' in viewer) ||
    !viewer.scene ||
    !Number.isFinite(at?.lat) ||
    !Number.isFinite(at?.lon)
  ) {
    return () => {};
  }

  const v = viewer as GuardViewer;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let done = false;
  const canvas = v.scene?.canvas;
  const documentRef =
    canvas && typeof canvas === 'object' && 'ownerDocument' in canvas
      ? (canvas.ownerDocument as
          | (EventTarget & { addEventListener?: EventTarget['addEventListener'] })
          | null)
      : null;
  const removers: Array<() => void> = [];
  let release = () => {};

  const stop = () => {
    if (done) return;
    done = true;
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
    for (const type of ['pointerdown', 'wheel'] as const) {
      canvas?.removeEventListener?.(type, stop);
    }
    documentRef?.removeEventListener?.('keydown', stop, true);
    for (const remove of removers) {
      remove();
    }
    release();
  };

  release = ownCameraArrival(viewer as object, stop);

  // Direct camera API callers (including duration-zero flights) must yield
  // before moving the eye. Restore the exact method ownership on completion.
  const cam = v.camera ?? v.scene?.camera;
  if (cam && typeof cam === 'object') {
    const camRecord = cam as Record<string, unknown>;
    for (const name of [
      'flyTo',
      'flyToBoundingSphere',
      'setView',
      'lookAt',
      'cancelFlight',
    ] as const) {
      const original = camRecord[name];
      if (typeof original !== 'function') continue;
      const descriptor = Object.getOwnPropertyDescriptor(cam, name);
      const wrapped = function (this: unknown, ...args: unknown[]) {
        stop();
        return (original as (...a: unknown[]) => unknown).apply(this, args);
      };
      camRecord[name] = wrapped;
      removers.push(() => {
        if (camRecord[name] !== wrapped) return;
        if (descriptor) {
          Object.defineProperty(cam, name, descriptor);
        } else {
          delete camRecord[name];
        }
      });
    }
  }

  const removeTracking = v.trackedEntityChanged?.addEventListener?.(stop);
  if (typeof removeTracking === 'function') {
    removers.push(removeTracking);
  }

  documentRef?.addEventListener?.('keydown', stop, true);
  for (const type of ['pointerdown', 'wheel'] as const) {
    canvas?.addEventListener?.(type, stop, { once: true, passive: true });
  }

  let remaining = attempts;
  const attempt = () => {
    if (done) return;
    if (isStale() || v.isDestroyed?.()) {
      stop();
      return;
    }
    remaining -= 1;
    const camera = v.camera ?? v.scene?.camera;
    const carto = camera?.positionCartographic;
    if (!camera || !carto) {
      if (remaining <= 0) {
        stop();
        return;
      }
      timer = setTimeout(attempt, intervalMs);
      return;
    }

    // The eye must clear both the framed target and the ground beneath it.
    const below = {
      lat: typeof carto.latitude === 'number' ? CesiumMath.toDegrees(carto.latitude) : at.lat,
      lon: typeof carto.longitude === 'number' ? CesiumMath.toDegrees(carto.longitude) : at.lon,
    };
    const { heightM, sampled } = sampleSurfaceM(v.scene, [{ lat: at.lat, lon: at.lon }, below]);

    if (sampled > 0 && Number.isFinite(heightM)) {
      const deficit = groundClearanceDeficitM(carto.height, heightM, clearanceM);
      if (deficit > 0) {
        stop();
        const longRad =
          typeof carto.longitude === 'number' ? carto.longitude : CesiumMath.toRadians(at.lon);
        const latRad =
          typeof carto.latitude === 'number' ? carto.latitude : CesiumMath.toRadians(at.lat);
        camera.flyTo?.({
          destination: Cartesian3.fromRadians(longRad, latRad, carto.height + deficit),
          orientation: {
            heading: camera.heading ?? 0,
            pitch: camera.pitch ?? 0,
            roll: camera.roll ?? 0,
          },
          duration: 0.9,
          easingFunction: EasingFunction?.CUBIC_IN_OUT ?? ((t: number) => t),
        });
        try {
          onLift?.(deficit);
        } catch {
          /* observer only */
        }
      }
      // Both samples answered: the surface is measured, nothing left to wait for.
      if (sampled === 2 || deficit > 0) {
        stop();
        return;
      }
    }
    if (remaining <= 0) {
      stop();
      return;
    }
    timer = setTimeout(attempt, intervalMs);
  };

  timer = setTimeout(attempt, intervalMs);
  return stop;
}
