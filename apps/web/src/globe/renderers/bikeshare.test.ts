import { Cartesian3 } from '@cesium/core';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { BikeshareStation } from '../../contracts';
import { quakeSnapshot } from '../../test/fakes';
import { DEFAULT_PALETTE } from '../tokens';
import { BikeshareRenderer } from './bikeshare';
import { type RenderContext, toMapColors } from './types';

describe('BikeshareRenderer', () => {
  let primitives: {
    add: <T>(c: T) => T;
    remove: (c: unknown) => unknown;
    collections: unknown[];
  };
  let ctx: RenderContext;
  let renderer: BikeshareRenderer;

  beforeEach(() => {
    const collections: unknown[] = [];
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

    renderer = new BikeshareRenderer(
      primitives as unknown as import('@cesium/engine').PrimitiveCollection,
      ctx,
    );
  });

  const stations: BikeshareStation[] = [
    {
      id: 'citibike:1',
      stationId: '1',
      systemId: 'citibike',
      name: 'Station 1',
      position: { lon: -74.006, lat: 40.7128 },
      bikesAvailable: 10,
      docksAvailable: 5,
      isInstalled: true,
      isRenting: true,
      isReturning: true,
    },
    {
      id: 'citibike:2',
      stationId: '2',
      systemId: 'citibike',
      name: 'Station 2',
      position: { lon: -74.01, lat: 40.72 },
      bikesAvailable: 0,
      docksAvailable: 15,
      isInstalled: true,
      isRenting: true,
      isReturning: true,
    },
  ];

  it('renders stations from snapshot and handles selection', () => {
    const snapshot = quakeSnapshot([], {
      feed: 'bikeshare',
      items: { kind: 'bikeshare', items: stations },
    });

    renderer.apply(snapshot);
    expect(ctx.requestRender).toHaveBeenCalled();

    // Locate station 1
    const loc = renderer.locate('citibike:1');
    expect(loc).toBeDefined();
    expect(loc?.symbolPx).toBe(32);

    // Set selection
    renderer.setSelected('citibike:1');
    expect(ctx.requestRender).toHaveBeenCalledTimes(2);

    // Clear
    renderer.clear();
    expect(renderer.locate('citibike:1')).toBeUndefined();
  });
});
