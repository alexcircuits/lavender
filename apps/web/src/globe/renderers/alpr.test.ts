import { Cartesian3, type Color } from '@cesium/core';
import type { PointPrimitiveCollection } from '@cesium/engine';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { AlprCamera } from '../../contracts';
import { DEFAULT_PALETTE } from '../tokens';
import { AlprRenderer, alprDirectionLabel, filterAndCapAlprCameras } from './alpr';
import { type RenderContext, toMapColors } from './types';

class MockPoints {
  readonly points: Array<{ position: Cartesian3; color: Color; pixelSize: number; id: unknown }> =
    [];
  add(o: { position: Cartesian3; color: Color; pixelSize: number; id: unknown }) {
    const p = { ...o };
    this.points.push(p);
    return p;
  }
  remove(p: unknown) {
    const i = this.points.indexOf(p as (typeof this.points)[0]);
    if (i !== -1) this.points.splice(i, 1);
  }
}

function cam(id: number, lat: number, lon: number): AlprCamera {
  return { id: `alpr:${id}`, osmId: BigInt(id), position: { lat, lon } };
}

const BOX = { west: -10, south: -10, east: 10, north: 10 };

describe('filterAndCapAlprCameras', () => {
  it('drops cameras outside the bbox', () => {
    const out = filterAndCapAlprCameras([cam(1, 0, 0), cam(2, 50, 0)], BOX);
    expect(out.map((c) => c.id)).toEqual(['alpr:1']);
  });

  it('keeps the nearest to center when capped', () => {
    const out = filterAndCapAlprCameras([cam(1, 9, 9), cam(2, 0, 1), cam(3, 0, 0)], BOX, 2);
    expect(out.map((c) => c.id)).toEqual(['alpr:3', 'alpr:2']);
  });

  it('handles antimeridian boxes', () => {
    const box = { west: 170, south: -10, east: -170, north: 10 };
    const out = filterAndCapAlprCameras([cam(1, 0, 175), cam(2, 0, -175), cam(3, 0, 0)], box);
    expect(out.map((c) => c.id).sort()).toEqual(['alpr:1', 'alpr:2']);
  });
});

describe('alprDirectionLabel', () => {
  it('labels bearings', () => {
    expect(alprDirectionLabel(0)).toBe('N (0°)');
    expect(alprDirectionLabel(100)).toBe('E (100°)');
    expect(alprDirectionLabel(359)).toBe('N (359°)');
    expect(alprDirectionLabel(undefined)).toBeUndefined();
  });
});

describe('AlprRenderer', () => {
  let ctx: RenderContext;
  let points: MockPoints;
  let renderer: AlprRenderer;

  beforeEach(() => {
    points = new MockPoints();
    const primitives = { add: <T>(c: T) => c, remove: () => true };
    ctx = {
      colors: toMapColors(DEFAULT_PALETTE),
      requestRender: vi.fn(),
      cameraPosition: () => Cartesian3.ZERO,
      cameraHeightM: () => 500_000,
      canvasSizePx: () => ({ width: 800, height: 600 }),
    };
    renderer = new AlprRenderer(
      primitives as unknown as import('@cesium/engine').PrimitiveCollection,
      ctx,
      points as unknown as PointPrimitiveCollection,
    );
  });

  it('renders, selects, locates and clears', () => {
    renderer.setCameras([cam(1, 0, 0), cam(2, 1, 1)]);
    expect(points.points).toHaveLength(2);
    expect(renderer.getCamera('alpr:1')).toBeDefined();
    expect(renderer.locate('alpr:2')).toBeDefined();
    renderer.setSelected('alpr:2');
    expect(points.points[1]?.pixelSize).toBe(14);
    expect(points.points[0]?.pixelSize).toBe(8);
    renderer.clear();
    expect(points.points).toHaveLength(0);
    expect(renderer.getCamera('alpr:1')).toBeUndefined();
  });
});
