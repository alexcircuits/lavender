import { Cartesian3, type Color } from '@cesium/core';
import type { PointPrimitiveCollection, PolylineCollection } from '@cesium/engine';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Route } from '../../contracts';
import { DEFAULT_PALETTE } from '../tokens';
import { DirectionsRenderer } from './directions';
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

describe('DirectionsRenderer', () => {
  let primitives: {
    add: <T>(c: T) => T;
    remove: (c: unknown) => unknown;
    collections: unknown[];
  };
  let ctx: RenderContext;
  let lines: MockPolylineCollection;
  let points: MockPointCollection;
  let renderer: DirectionsRenderer;

  beforeEach(() => {
    primitives = {
      collections: [],
      add: (c) => {
        primitives.collections.push(c);
        return c;
      },
      remove: (c) => {
        const idx = primitives.collections.indexOf(c);
        if (idx !== -1) primitives.collections.splice(idx, 1);
      },
    };
    ctx = {
      colors: toMapColors(DEFAULT_PALETTE),
      requestRender: vi.fn(),
      cameraPosition: () => Cartesian3.ZERO,
      cameraHeightM: () => 1000,
    };
    lines = new MockPolylineCollection();
    points = new MockPointCollection();
    renderer = new DirectionsRenderer(
      primitives as unknown as import('@cesium/engine').PrimitiveCollection,
      ctx,
      lines as unknown as PolylineCollection,
      points as unknown as PointPrimitiveCollection,
    );
  });

  it('draws origin and destination waypoints', () => {
    renderer.setWaypoints([13.4, 52.5], [13.41, 52.51]);
    expect(points.points.length).toBe(2);
    expect(ctx.requestRender).toHaveBeenCalled();

    const originLoc = renderer.locateWaypoint('origin');
    expect(originLoc).not.toBeNull();
    expect(originLoc?.symbolPx).toBe(14);

    const destLoc = renderer.locateWaypoint('destination');
    expect(destLoc).not.toBeNull();
    expect(destLoc?.symbolPx).toBe(14);
  });

  it('draws route polyline and step maneuver points', () => {
    const route: Route = {
      ok: true,
      profile: 'car',
      distanceM: 1500,
      durationS: 300,
      geometry: [
        [13.4, 52.5],
        [13.405, 52.505],
        [13.41, 52.51],
      ],
      steps: [
        {
          index: 0,
          type: 'depart',
          name: 'Unter den Linden',
          distanceM: 750,
          durationS: 150,
          lon: 13.4,
          lat: 52.5,
          instruction: 'Head east on Unter den Linden',
        },
        {
          index: 1,
          type: 'arrive',
          name: 'Alexanderplatz',
          distanceM: 750,
          durationS: 150,
          lon: 13.41,
          lat: 52.51,
          instruction: 'Arrive at Alexanderplatz',
        },
      ],
    };

    renderer.setRoute(route);
    expect(lines.lines.length).toBe(1);
    expect(lines.lines[0]?.positions.length).toBe(3);
    expect(points.points.length).toBe(2);
    expect(renderer.getRoute()).toBe(route);

    const step0Loc = renderer.locateStep(0);
    expect(step0Loc).not.toBeNull();
    expect(step0Loc?.symbolPx).toBe(12);

    renderer.setSelectedStep(1);
    expect(ctx.requestRender).toHaveBeenCalled();
  });

  it('clears route and waypoints on clear and destroy', () => {
    renderer.setWaypoints([13.4, 52.5], [13.41, 52.51]);
    renderer.setRoute({
      ok: true,
      profile: 'foot',
      distanceM: 500,
      durationS: 300,
      geometry: [
        [13.4, 52.5],
        [13.41, 52.51],
      ],
    });

    expect(lines.lines.length).toBe(1);
    expect(points.points.length).toBe(2);

    renderer.clear();
    expect(lines.lines.length).toBe(0);
    expect(points.points.length).toBe(0);

    renderer.destroy();
    expect(primitives.collections.length).toBe(0);
  });
});
