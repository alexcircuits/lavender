import type { Scene } from '@cesium/engine';
import { describe, expect, it, vi } from 'vitest';
import { cesiumCameraPort } from './cesiumCamera';

interface Flight {
  duration: number;
  complete: () => void;
  cancel: () => void;
}

function createMockScene() {
  const postRenderListeners = new Set<() => void>();
  let current: Flight | null = null;
  let destroyed = false;
  const camera = {
    positionCartographic: { height: 10_000 },
    pitch: -0.5,
    positionWC: { x: 1, y: 2, z: 3 },
    cancelFlight: vi.fn(() => {
      const canceled = current;
      current = null;
      canceled?.cancel();
    }),
    flyTo: vi.fn((options: Flight) => {
      current = options;
      // Cesium's Camera.flyTo calls complete synchronously after setView for duration <= 0.
      if (options.duration <= 0) {
        current = null;
        options.complete();
      }
    }),
    zoomIn: vi.fn(),
    zoomOut: vi.fn(),
    moveEnd: { raiseEvent: vi.fn() },
  };
  const scene = {
    camera,
    isDestroyed: () => destroyed,
    requestRender: vi.fn(),
    postRender: {
      addEventListener: (cb: () => void) => {
        postRenderListeners.add(cb);
        return () => postRenderListeners.delete(cb);
      },
    },
  };
  return {
    scene: scene as unknown as Scene,
    camera,
    requestRender: scene.requestRender,
    postRender: () => {
      for (const listener of postRenderListeners) listener();
    },
    postRenderListenerCount: () => postRenderListeners.size,
    flight: () => camera.flyTo.mock.calls.at(-1)?.[0],
    destroy: () => {
      destroyed = true;
    },
  };
}

const TARGET = { kind: 'point', lat: 20, lon: -60, heightM: 400_000 } as const;

describe('cesiumCameraPort', () => {
  it('reads height and pitch from Cesium camera', () => {
    const { scene } = createMockScene();
    const port = cesiumCameraPort(scene);
    expect(port.heightM()).toBe(10_000);
    expect(port.pitchRad()).toBe(-0.5);
  });

  it('requests frames only while flying and settles once without raising SDK events', () => {
    const mock = createMockScene();
    const settled = vi.fn();
    const port = cesiumCameraPort(mock.scene, settled);
    port.flyTo(TARGET, 1.2);
    expect(settled).not.toHaveBeenCalled();
    mock.requestRender.mockClear();
    mock.postRender();
    expect(mock.requestRender).toHaveBeenCalledTimes(1);
    mock.flight()?.complete();
    expect(settled).toHaveBeenCalledTimes(1);
    expect(mock.requestRender).toHaveBeenCalledTimes(2);
    mock.flight()?.complete();
    mock.flight()?.cancel();
    mock.postRender();
    expect(mock.requestRender).toHaveBeenCalledTimes(2);
    expect(settled).toHaveBeenCalledTimes(1);
    expect(mock.camera.moveEnd.raiseEvent).not.toHaveBeenCalled();
  });

  it('settles synchronous zero-duration fly and orientation exactly once', () => {
    const mock = createMockScene();
    const settled = vi.fn();
    const port = cesiumCameraPort(mock.scene, settled);
    port.flyTo(TARGET, 0);
    expect(settled).toHaveBeenCalledTimes(1);
    port.orient(0.5, -1, 0);
    expect(settled).toHaveBeenCalledTimes(2);
    mock.requestRender.mockClear();
    mock.postRender();
    expect(mock.requestRender).not.toHaveBeenCalled();
    expect(mock.camera.moveEnd.raiseEvent).not.toHaveBeenCalled();
  });

  it('stops frame requests for an externally canceled flight without publishing its pose', () => {
    const mock = createMockScene();
    const settled = vi.fn();
    const port = cesiumCameraPort(mock.scene, settled);
    port.flyTo(TARGET, 1.2);
    mock.camera.cancelFlight();
    // Whoever cancelled owns the next view (restoreCamera, or Cesium's moveEnd after input).
    expect(settled).not.toHaveBeenCalled();
    mock.requestRender.mockClear();
    mock.postRender();
    expect(mock.requestRender).not.toHaveBeenCalled();
    mock.flight()?.complete();
    expect(settled).not.toHaveBeenCalled();
  });

  it('ends a flight Cesium rejects, so frames are not requested forever, and rethrows', () => {
    const mock = createMockScene();
    const settled = vi.fn();
    const port = cesiumCameraPort(mock.scene, settled);
    const rejected = new Error('DeveloperError: bad destination');
    mock.camera.flyTo.mockImplementationOnce(() => {
      throw rejected;
    });
    expect(() => port.flyTo(TARGET, 1.2)).toThrow(rejected);
    mock.camera.flyTo.mockImplementationOnce(() => {
      throw rejected;
    });
    expect(() => port.orient(0.5, -1, 0.6)).toThrow(rejected);
    mock.requestRender.mockClear();
    mock.postRender();
    expect(mock.requestRender).not.toHaveBeenCalled();
    expect(settled).not.toHaveBeenCalled();
    // The next flight still animates and settles normally.
    port.flyTo(TARGET, 1.2);
    mock.postRender();
    expect(mock.requestRender).toHaveBeenCalled();
    mock.flight()?.complete();
    expect(settled).toHaveBeenCalledTimes(1);
  });

  it('removes its frame hook on destroy and ignores the flight it leaves behind', () => {
    const mock = createMockScene();
    const settled = vi.fn();
    const port = cesiumCameraPort(mock.scene, settled);
    port.flyTo(TARGET, 1.2);
    expect(mock.postRenderListenerCount()).toBe(1);
    port.destroy();
    expect(mock.postRenderListenerCount()).toBe(0);
    mock.requestRender.mockClear();
    mock.flight()?.complete();
    mock.flight()?.cancel();
    expect(settled).not.toHaveBeenCalled();
    expect(mock.requestRender).not.toHaveBeenCalled();
    port.destroy();
  });

  it('ignores canceled or late callbacks from a superseded flight', () => {
    const mock = createMockScene();
    const settled = vi.fn();
    const port = cesiumCameraPort(mock.scene, settled);
    port.flyTo(TARGET, 1.2);
    const old = mock.flight();
    port.orient(0.5, -1, 0.6);
    expect(settled).not.toHaveBeenCalled();
    old?.complete();
    old?.cancel();
    expect(settled).not.toHaveBeenCalled();
    mock.requestRender.mockClear();
    mock.postRender();
    expect(mock.requestRender).toHaveBeenCalledTimes(1);
    mock.flight()?.complete();
    expect(settled).toHaveBeenCalledTimes(1);
  });

  it('zooms immediately, superseding a flight without an intermediate settlement', () => {
    const mock = createMockScene();
    const settled = vi.fn();
    const port = cesiumCameraPort(mock.scene, settled);
    port.flyTo(TARGET, 1.2);
    port.zoom(500);
    expect(mock.camera.zoomIn).toHaveBeenCalledWith(500);
    expect(settled).toHaveBeenCalledTimes(1);
    port.zoom(-200);
    expect(mock.camera.zoomOut).toHaveBeenCalledWith(200);
    expect(settled).toHaveBeenCalledTimes(2);
    expect(mock.camera.moveEnd.raiseEvent).not.toHaveBeenCalled();
  });

  it('does not notify or request frames after the scene is destroyed', () => {
    const mock = createMockScene();
    const settled = vi.fn();
    const port = cesiumCameraPort(mock.scene, settled);
    port.flyTo(TARGET, 1.2);
    mock.destroy();
    mock.requestRender.mockClear();
    mock.flight()?.complete();
    mock.postRender();
    expect(mock.requestRender).not.toHaveBeenCalled();
    expect(settled).not.toHaveBeenCalled();
  });
});
