import { Cartesian3, type Color } from '@cesium/core';
import type { PointPrimitiveCollection, PolylineCollection } from '@cesium/engine';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { TransitTrail, TransitVehicle } from '../../contracts';
import { quakeSnapshot } from '../../test/fakes';
import { DEFAULT_PALETTE } from '../tokens';
import { TransitRenderer } from './transit';
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
  readonly lines: Array<{ positions: Cartesian3[]; width: number }> = [];
  add(options: { positions: Cartesian3[]; width: number }) {
    const line = { ...options };
    this.lines.push(line);
    return line;
  }
  remove(line: unknown) {
    const idx = this.lines.indexOf(line as (typeof this.lines)[0]);
    if (idx !== -1) this.lines.splice(idx, 1);
  }
}

describe('TransitRenderer', () => {
  let primitives: {
    add: <T>(c: T) => T;
    remove: (c: unknown) => unknown;
    collections: unknown[];
  };
  let ctx: RenderContext;
  let lines: MockPolylineCollection;
  let points: MockPointCollection;
  let renderer: TransitRenderer;

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

    renderer = new TransitRenderer(
      primitives as unknown as import('@cesium/engine').PrimitiveCollection,
      ctx,
      lines as unknown as PolylineCollection,
      points as unknown as PointPrimitiveCollection,
    );
  });

  const vehicles: TransitVehicle[] = [
    {
      id: 'mbta:101',
      vehicleId: '101',
      feedId: 'mbta',
      position: { lon: -71.0589, lat: 42.3601 },
      bearing: 180,
      speedMps: 12.5,
      timestampSource: 'vehicle',
      routeId: '1',
      label: 'Bus 101',
      mode: 'bus',
    },
  ];

  it('renders transit vehicles and draws trails with Zig WASM RDP', () => {
    const snapshot = quakeSnapshot([], {
      feed: 'transit',
      items: { kind: 'transit', items: vehicles },
    });

    renderer.apply(snapshot);
    expect(ctx.requestRender).toHaveBeenCalled();

    // Locate vehicle
    const loc = renderer.locate('mbta:101');
    expect(loc).toBeDefined();

    // Trail with fixes: [time_ms, lat, lon, flags, epoch_id]
    const trail: TransitTrail = {
      version: 1,
      feedId: 'mbta',
      vehicleId: '101',
      fixes: [
        [1700000000000, 42.36, -71.058, 0, 1],
        [1700000010000, 42.3601, -71.0589, 0, 1],
      ],
      epochs: [],
      truncated: false,
    };

    renderer.showTrail(trail);
    expect(ctx.requestRender).toHaveBeenCalledTimes(2);

    renderer.setSelected('mbta:101');
    expect(ctx.requestRender).toHaveBeenCalledTimes(3);

    renderer.clear();
    expect(renderer.locate('mbta:101')).toBeUndefined();
  });
});
