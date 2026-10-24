import { Cartesian3, type Color } from '@cesium/core';
import type { PointPrimitiveCollection, PolylineCollection } from '@cesium/engine';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { CameraSource, CctvDetectionsResponse } from '../../contracts';
import { DEFAULT_PALETTE } from '../tokens';
import { CctvRenderer } from './cctv';
import { type RenderContext, toMapColors } from './types';

class MockPointCollection {
  readonly points: Array<{ position: Cartesian3; color: Color; pixelSize: number; id: unknown }> =
    [];
  add(options: { position: Cartesian3; color: Color; pixelSize: number; id: unknown }) {
    const point = { ...options };
    this.points.push(point);
    return point;
  }
  remove(point: unknown) {
    const idx = this.points.indexOf(point as (typeof this.points)[0]);
    if (idx !== -1) this.points.splice(idx, 1);
  }
  destroy() {
    this.points.length = 0;
  }
}

class MockPolylineCollection {
  readonly lines: Array<{ positions: Cartesian3[]; width: number; material: unknown }> = [];
  add(options: { positions: Cartesian3[]; width: number; material: unknown }) {
    const line = { ...options };
    this.lines.push(line);
    return line;
  }
  remove(line: unknown) {
    const idx = this.lines.indexOf(line as (typeof this.lines)[0]);
    if (idx !== -1) this.lines.splice(idx, 1);
  }
}

describe('CctvRenderer', () => {
  let primitives: {
    add: <T>(c: T) => T;
    remove: (c: unknown) => unknown;
    collections: unknown[];
  };
  let ctx: RenderContext;
  let lines: MockPolylineCollection;
  let points: MockPointCollection;
  let renderer: CctvRenderer;

  beforeEach(() => {
    lines = new MockPolylineCollection();
    points = new MockPointCollection();
    const collections: unknown[] = [lines, points];
    primitives = {
      collections,
      add: <T>(c: T) => {
        collections.push(c);
        return c;
      },
      remove: (c: unknown) => {
        const idx = collections.indexOf(c);
        if (idx !== -1) collections.splice(idx, 1);
        return c;
      },
    };

    const colors = toMapColors(DEFAULT_PALETTE);
    ctx = {
      colors,
      requestRender: vi.fn(),
      cameraPosition: () => Cartesian3.ZERO,
      cameraHeightM: () => 1000,
      canvasSizePx: () => ({ width: 1000, height: 800 }),
    };

    renderer = new CctvRenderer(
      primitives as unknown as import('@cesium/engine').PrimitiveCollection,
      ctx,
      lines as unknown as PolylineCollection,
      points as unknown as PointPrimitiveCollection,
    );
  });

  const cameras: CameraSource[] = [
    {
      id: 'cam:harbor:1',
      name: 'Harbor Gate North',
      city: 'Hamburg',
      cityId: 'hamburg',
      provider: 'HPA',
      lat: 53.54,
      lon: 9.98,
      headingDeg: 135,
      pitchDeg: -12,
      fovDeg: 70,
      rangeM: 800,
      feedType: 'hls',
      url: 'https://hamburg.de/cam1',
      snapshotUrl: 'https://hamburg.de/cam1.jpg',
      license: 'Public',
      code: 'HAM-01',
      sourceKind: 'port-authority',
    },
  ];

  it('renders cameras, draws 3D frustum with Zig WASM, and accepts Mojo detections', () => {
    renderer.setCameras(cameras);
    expect(ctx.requestRender).toHaveBeenCalled();
    expect(points.points.length).toBe(1);

    // Select camera: generates 8 wireframe lines (4 rays + 4 edges) via Zig WASM
    renderer.setSelected('cam:harbor:1');
    expect(lines.lines.length).toBe(8);

    // Associate Mojo detections
    const detections: CctvDetectionsResponse = {
      cameraId: 'cam:harbor:1',
      timestampMs: 1_700_000_000_000n,
      frameSequence: 1024n,
      engine: 'mojo-mlir-simd',
      detections: [
        {
          bbox: [0.1, 0.2, 0.6, 0.7],
          classId: 3,
          className: 'vessel',
          confidence: 0.94,
        },
      ],
    };

    renderer.setDetections(detections);
    expect(renderer.getDetections()).toEqual(detections);

    // Locate camera
    const loc = renderer.locate('cam:harbor:1');
    expect(loc).toBeDefined();

    // Deselect and clear
    renderer.setSelected(null);
    expect(lines.lines.length).toBe(0);

    renderer.clear();
    expect(renderer.locate('cam:harbor:1')).toBeUndefined();
  });
});
