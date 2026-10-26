import { Cartesian3, type Color } from '@cesium/core';
import type { LabelCollection, PointPrimitiveCollection } from '@cesium/engine';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { RadioStation } from '../../contracts';
import { DEFAULT_PALETTE } from '../tokens';
import {
  RadioRenderer,
  radioCategoryColor,
  radioSingletonLabelLimit,
  reconcileRadioClusterCandidates,
  selectRadioSingletonCandidates,
} from './radio';
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

class MockLabels {
  readonly labels: Array<{ text: string; show: boolean; id: unknown }> = [];
  add(o: { text: string; show: boolean; id: unknown }) {
    const l = { ...o };
    this.labels.push(l);
    return l;
  }
  remove(l: unknown) {
    const i = this.labels.indexOf(l as (typeof this.labels)[0]);
    if (i !== -1) this.labels.splice(i, 1);
  }
}

function station(id: string, lon: number, category: RadioStation['category']): RadioStation {
  return {
    id,
    name: `Station ${id}`,
    streamUrl: 'https://example.com/s',
    countryCode: 'US',
    tags: [],
    category,
    codec: 'MP3',
    bitrateKbps: 64,
    votes: 0,
    clickCount: 0,
    position: { lat: 10, lon },
  };
}

describe('radio helpers', () => {
  it('limits singleton labels by camera height', () => {
    expect(radioSingletonLabelLimit(5_000_000)).toBe(16);
    expect(radioSingletonLabelLimit(2_000_000)).toBe(16);
    expect(radioSingletonLabelLimit(500_000)).toBe(32);
    expect(radioSingletonLabelLimit(1_000)).toBe(48);
    expect(radioSingletonLabelLimit(Number.NaN)).toBe(48);
  });

  it('maps categories to class colors', () => {
    expect(radioCategoryColor('emergency')).toBe('#f85149');
    expect(radioCategoryColor('aviation')).toBe('#e3b341');
    expect(radioCategoryColor('news')).toBe('#58a6ff');
    expect(radioCategoryColor('talk')).toBe('#d2a8ff');
    expect(radioCategoryColor('general')).toBe('#3fb950');
  });

  it('ranks singletons by distance then id', () => {
    const picked = selectRadioSingletonCandidates(
      [
        { id: 'b', distanceM: 5 },
        { id: 'a', distanceM: 5 },
        { id: 'c', distanceM: 1 },
        { id: 'd' },
      ],
      3,
    );
    expect(picked.map((c) => c.id)).toEqual(['c', 'a', 'b']);
  });

  it('reconciles cluster identity by greatest overlap', () => {
    const out = reconcileRadioClusterCandidates(
      [
        { id: 'm1', stationIds: ['1', '2', '3'] },
        { id: 'm2', stationIds: ['9'] },
      ],
      [{ identityId: 'keep', stationIds: ['1', '2'] }],
      (_c, i) => `new-${i}`,
    );
    expect(out[0]?.identityId).toBe('keep');
    expect(out[1]?.identityId).toBe('new-1');
  });
});

describe('RadioRenderer', () => {
  let ctx: RenderContext;
  let points: MockPoints;
  let labels: MockLabels;
  let renderer: RadioRenderer;

  beforeEach(() => {
    points = new MockPoints();
    labels = new MockLabels();
    const collections: unknown[] = [points, labels];
    const primitives = {
      add: <T>(c: T) => {
        collections.push(c);
        return c;
      },
      remove: (c: unknown) => {
        const i = collections.indexOf(c);
        if (i !== -1) collections.splice(i, 1);
        return true;
      },
    };
    ctx = {
      colors: toMapColors(DEFAULT_PALETTE),
      requestRender: vi.fn(),
      cameraPosition: () => Cartesian3.fromDegrees(0, 10, 1_000_000),
      cameraHeightM: () => 5_000_000,
      canvasSizePx: () => ({ width: 800, height: 600 }),
    };
    renderer = new RadioRenderer(
      primitives as unknown as import('@cesium/engine').PrimitiveCollection,
      ctx,
      points as unknown as PointPrimitiveCollection,
      labels as unknown as LabelCollection,
    );
  });

  it('renders stations and bounds visible labels', () => {
    const stations = Array.from({ length: 20 }, (_, i) => station(`s${i}`, i, 'news'));
    renderer.setStations(stations);
    expect(points.points).toHaveLength(20);
    expect(labels.labels.filter((l) => l.show)).toHaveLength(16);
    expect(renderer.getStation('s3')).toBeDefined();
    expect(renderer.locate('s3')).toBeDefined();
    expect(renderer.locate('nope')).toBeUndefined();
  });

  it('styles selection and clears', () => {
    renderer.setStations([station('a', 0, 'talk'), station('b', 1, 'talk')]);
    renderer.setSelected('b');
    expect(points.points[1]?.pixelSize).toBe(13);
    expect(points.points[0]?.pixelSize).toBe(7);
    renderer.clear();
    expect(points.points).toHaveLength(0);
    expect(labels.labels).toHaveLength(0);
    expect(renderer.getStation('a')).toBeUndefined();
  });
});
