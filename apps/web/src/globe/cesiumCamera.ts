/** `CameraPort` over a Cesium scene camera (see camera.ts for the behaviour). */
import { Cartesian3, Math as CesiumMath, Rectangle } from '@cesium/core';
import type { Scene } from '@cesium/engine';
import { type CameraPort, type Destination, standardEasing } from './camera';

const TOP_DOWN = { heading: 0, pitch: -CesiumMath.PI_OVER_TWO, roll: 0 };

function toCesiumDestination(destination: Destination): Cartesian3 | Rectangle {
  if (destination.kind === 'point') {
    return Cartesian3.fromDegrees(destination.lon, destination.lat, destination.heightM);
  }
  return Rectangle.fromDegrees(
    destination.west,
    destination.south,
    destination.east,
    destination.north,
  );
}

/** The Cesium camera port; `destroy` removes its frame hook and ignores pending flights. */
export interface CesiumCameraPort extends CameraPort {
  destroy(): void;
}

/**
 * Commanded moves call `onSettled` once when they complete. A cancelled flight only stops its
 * frame requests: whoever cancelled it owns the next view (a newer move, restoreCamera, or
 * Cesium's own moveEnd after mouse input), so its mid-flight pose is never published.
 */
export function cesiumCameraPort(scene: Scene, onSettled: () => void = () => {}): CesiumCameraPort {
  const camera = scene.camera;
  let inFlight = false;
  let generation = 0;

  // requestRenderMode draws only on request; keep frames coming while a flight animates.
  const offPostRender = scene.postRender.addEventListener(() => {
    if (inFlight && !scene.isDestroyed()) {
      scene.requestRender();
    }
  });

  const begin = (durationS: number) => {
    // Invalidate the old callbacks before Cesium synchronously cancels its flight.
    const current = ++generation;
    camera.cancelFlight();
    inFlight = durationS > 0;
    let ended = false;
    const end = (completed: boolean) => {
      if (current !== generation || ended) return;
      ended = true;
      inFlight = false;
      if (!completed || scene.isDestroyed()) return;
      scene.requestRender();
      onSettled();
    };
    scene.requestRender();
    return { complete: () => end(true), cancel: () => end(false) };
  };

  /** A Cesium flight; if Cesium rejects it, the move ends (no endless frames) and it rethrows. */
  const fly = (
    destination: Cartesian3 | Rectangle,
    orientation: { heading: number; pitch: number; roll: number },
    durationS: number,
  ) => {
    const move = begin(durationS);
    try {
      camera.flyTo({
        destination,
        orientation,
        duration: durationS,
        easingFunction: standardEasing,
        complete: move.complete,
        cancel: move.cancel,
      });
    } catch (error) {
      move.cancel();
      throw error;
    }
  };

  return {
    heightM: () => camera.positionCartographic.height,
    pitchRad: () => camera.pitch,
    flyTo(destination, durationS) {
      fly(toCesiumDestination(destination), TOP_DOWN, durationS);
    },
    orient(headingRad, pitchRad, durationS) {
      fly(
        Cartesian3.clone(camera.positionWC),
        { heading: headingRad, pitch: pitchRad, roll: 0 },
        durationS,
      );
    },
    zoom(amountM) {
      const move = begin(0);
      if (amountM >= 0) camera.zoomIn(amountM);
      else camera.zoomOut(-amountM);
      move.complete();
    },
    destroy() {
      generation += 1;
      inFlight = false;
      offPostRender();
    },
  };
}
