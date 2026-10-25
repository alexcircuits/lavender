import { Cartesian3, type Color } from '@cesium/core';
import type { LabelCollection, PointPrimitiveCollection } from '@cesium/engine';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { AirQualityReading } from '../../contracts';
import { DEFAULT_PALETTE } from '../tokens';
import { AirQualityRenderer } from './airQuality';
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

class MockLabelCollection {
  readonly labels: Array<{ position: Cartesian3; text: string; id: unknown }> = [];
  add(options: { position: Cartesian3; text: string; id: unknown }) {
    const label = { ...options };
    this.labels.push(label);
    return label;
  }
  remove(label: unknown) {
    const idx = this.labels.indexOf(label as (typeof this.labels)[0]);
    if (idx !== -1) this.labels.splice(idx, 1);
  }
  destroy() {
    this.labels.length = 0;
  }
}

const SAMPLE_READINGS: AirQualityReading[] = [
  {
    id: 'eccc:station_1',
    provider: 'eccc',
    scale: 'AQHI',
    value: 2,
    band: 'low',
    stationId: 's1',
    name: 'Station 1',
    zone: 'ont',
    latitude: 45.4215,
    longitude: -75.6972,
    observedAt: '2026-10-25T12:00:00Z',
    color: '#00ccff',
    label: '2',
    risk: 'Low Risk',
  },
  {
    id: 'eccc:station_2',
    provider: 'eccc',
    scale: 'AQHI',
    value: 7,
    band: 'high',
    stationId: 's2',
    name: 'Station 2',
    zone: 'que',
    latitude: 46.8139,
    longitude: -71.208,
    observedAt: '2026-10-25T12:00:00Z',
    color: '#ff6600',
    label: '7',
    risk: 'High Risk',
  },
];

describe('AirQualityRenderer', () => {
  let primitives: {
    add: <T>(c: T) => T;
    remove: (c: unknown) => unknown;
    collections: unknown[];
  };
  let ctx: RenderContext;
  let points: MockPointCollection;
  let labels: MockLabelCollection;
  let renderer: AirQualityRenderer;

  beforeEach(() => {
    points = new MockPointCollection();
    labels = new MockLabelCollection();
    const collections: unknown[] = [points, labels];
    primitives = {
      collections,
      add: <T>(c: T) => {
        collections.push(c);
        return c;
      },
      remove: (c: unknown) => {
        const idx = collections.indexOf(c);
        if (idx !== -1) collections.splice(idx, 1);
        return true;
      },
    };

    ctx = {
      colors: toMapColors(DEFAULT_PALETTE),
      requestRender: vi.fn(),
      cameraPosition: () => Cartesian3.ZERO,
      cameraHeightM: () => 500_000,
      canvasSizePx: () => ({ width: 800, height: 600 }),
    };

    renderer = new AirQualityRenderer(
      primitives as unknown as import('@cesium/engine').PrimitiveCollection,
      ctx,
      points as unknown as PointPrimitiveCollection,
      labels as unknown as LabelCollection,
    );
  });

  it('renders station points and labels on setReadings', () => {
    renderer.setReadings(SAMPLE_READINGS);
    expect(points.points).toHaveLength(2);
    expect(labels.labels).toHaveLength(2);
    expect(ctx.requestRender).toHaveBeenCalled();
    expect(renderer.getReading('eccc:station_1')).toBeDefined();
    expect(renderer.locate('eccc:station_1')).toBeDefined();
  });

  it('updates selection styling correctly', () => {
    renderer.setReadings(SAMPLE_READINGS);
    renderer.setSelected('eccc:station_2');
    expect(points.points[1]?.pixelSize).toBe(14);
    expect(points.points[0]?.pixelSize).toBe(8);
  });

  it('clears all station primitives on clear and destroy', () => {
    renderer.setReadings(SAMPLE_READINGS);
    expect(points.points).toHaveLength(2);
    renderer.clear();
    expect(points.points).toHaveLength(0);
    expect(labels.labels).toHaveLength(0);
    expect(renderer.getReading('eccc:station_1')).toBeUndefined();
  });
});
