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

export function cesiumCameraPort(scene: Scene): CameraPort {
  const camera = scene.camera;
  return {
    heightM: () => camera.positionCartographic.height,
    pitchRad: () => camera.pitch,
    flyTo(destination, durationS) {
      camera.cancelFlight();
      camera.flyTo({
        destination: toCesiumDestination(destination),
        orientation: TOP_DOWN,
        duration: durationS,
        easingFunction: standardEasing,
      });
      scene.requestRender();
    },
    orient(headingRad, pitchRad, durationS) {
      camera.cancelFlight();
      camera.flyTo({
        destination: Cartesian3.clone(camera.positionWC),
        orientation: { heading: headingRad, pitch: pitchRad, roll: 0 },
        duration: durationS,
        easingFunction: standardEasing,
      });
      scene.requestRender();
    },
    zoom(amountM) {
      camera.cancelFlight();
      if (amountM >= 0) camera.zoomIn(amountM);
      else camera.zoomOut(-amountM);
      scene.requestRender();
    },
  };
}
