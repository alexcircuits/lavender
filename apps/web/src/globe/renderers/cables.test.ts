import { Cartesian3 } from '@cesium/core';
import { PointPrimitiveCollection, PolylineCollection } from '@cesium/engine';
import { describe, expect, it, vi } from 'vitest';
import { DEFAULT_PALETTE } from '../tokens';
import {
  CableRenderer,
  DEFAULT_CABLE_WIDTH,
  type LandingPointItem,
  SELECTED_CABLE_WIDTH,
  type SubmarineCableItem,
} from './cables';
import { type RenderContext, toMapColors } from './types';

function setup() {
  const polylines = new PolylineCollection();
  const points = new PointPrimitiveCollection();
  const requestRender = vi.fn();
  const colors = toMapColors(DEFAULT_PALETTE);
  const ctx: RenderContext = {
    colors,
    requestRender,
    cameraPosition: () => Cartesian3.ZERO,
    cameraHeightM: () => 1_000_000,
  };
  const renderer = new CableRenderer(polylines, points, ctx);
  return { polylines, points, renderer, requestRender, colors };
}

describe('CableRenderer', () => {
  it('adds and updates submarine cables', () => {
    const { polylines, renderer, requestRender } = setup();

    const sampleCables: SubmarineCableItem[] = [
      {
        id: 'atlantic-express-1',
        name: 'Atlantic Express 1',
        color: '#39d5ff',
        coordinates: [
          [
            [-74.0, 40.7],
            [-50.0, 45.0],
            [-0.1, 51.5],
          ],
        ],
      },
      {
        id: 'pacific-cross-2',
        name: 'Pacific Cross 2',
        coordinates: [
          [
            [139.7, 35.7],
            [-122.4, 37.8],
          ],
        ],
      },
    ];

    renderer.setCables(sampleCables);
    expect(renderer.cableCount).toBe(2);
    expect(polylines.length).toBe(2);
    expect(requestRender).toHaveBeenCalled();

    // Verify locate finds centroid
    const located = renderer.locate('atlantic-express-1');
    expect(located).toBeDefined();
    expect(located?.position).toBeDefined();
    expect(located?.symbolPx).toBe(24);

    // Prune one cable
    renderer.setCables(sampleCables.slice(0, 1));
    expect(renderer.cableCount).toBe(1);
    expect(polylines.length).toBe(1);
  });

  it('adds and removes coastal landing points', () => {
    const { points, renderer, requestRender } = setup();

    const sampleLandings: LandingPointItem[] = [
      {
        id: 'bude-uk',
        name: 'Bude, United Kingdom',
        position: { lon: -4.54, lat: 50.82 },
      },
      {
        id: 'halifax-canada',
        name: 'Halifax, NS, Canada',
        position: { lon: -63.57, lat: 44.64 },
      },
    ];

    renderer.setLandings(sampleLandings);
    expect(renderer.landingCount).toBe(2);
    expect(points.length).toBe(2);
    expect(requestRender).toHaveBeenCalled();

    // Verify locate finds landing point
    const located = renderer.locate('bude-uk');
    expect(located).toBeDefined();
    expect(located?.symbolPx).toBe(6);

    // Prune landings
    renderer.setLandings(sampleLandings.slice(0, 1));
    expect(renderer.landingCount).toBe(1);
    expect(points.length).toBe(1);
  });

  it('applies selection highlighting and reverts on deselection', () => {
    const { polylines, renderer, requestRender } = setup();

    const cable: SubmarineCableItem = {
      id: 'cable-alpha',
      name: 'Cable Alpha',
      coordinates: [
        [
          [10.0, 20.0],
          [30.0, 40.0],
        ],
      ],
    };

    renderer.setCables([cable]);
    const poly = polylines.get(0);
    expect(poly.width).toBe(DEFAULT_CABLE_WIDTH);

    // Select
    renderer.setSelected('cable-alpha');
    expect(poly.width).toBe(SELECTED_CABLE_WIDTH);
    expect(requestRender).toHaveBeenCalled();

    // Deselect
    renderer.setSelected(null);
    expect(poly.width).toBe(DEFAULT_CABLE_WIDTH);
  });

  it('clears and destroys all primitives', () => {
    const { polylines, points, renderer } = setup();

    renderer.setCables([
      {
        id: 'cable-1',
        name: 'Cable 1',
        coordinates: [
          [
            [0, 0],
            [1, 1],
          ],
        ],
      },
    ]);
    renderer.setLandings([
      {
        id: 'landing-1',
        name: 'Landing 1',
        position: { lon: 0, lat: 0 },
      },
    ]);

    expect(renderer.cableCount).toBe(1);
    expect(renderer.landingCount).toBe(1);

    renderer.clear();
    expect(renderer.cableCount).toBe(0);
    expect(renderer.landingCount).toBe(0);
    expect(polylines.length).toBe(0);
    expect(points.length).toBe(0);

    renderer.destroy();
    expect(renderer.cableCount).toBe(0);
  });
});
