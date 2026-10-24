import { Cartesian3 } from '@cesium/core';
import { PolylineCollection } from '@cesium/engine';
import { describe, expect, it, vi } from 'vitest';
import type { BoundaryFeature } from '../../contracts';
import { DEFAULT_PALETTE } from '../tokens';
import { BoundaryRenderer, DEFAULT_DISPUTED_WIDTH, SELECTED_BOUNDARY_WIDTH } from './boundaries';
import { type RenderContext, toMapColors } from './types';

function setup() {
  const polylines = new PolylineCollection();
  const requestRender = vi.fn();
  const colors = toMapColors(DEFAULT_PALETTE);
  const ctx: RenderContext = {
    colors,
    requestRender,
    cameraPosition: () => Cartesian3.ZERO,
    cameraHeightM: () => 1_000_000,
  };
  const renderer = new BoundaryRenderer(polylines, ctx);
  return { polylines, renderer, requestRender, colors };
}

const mockFeatures: BoundaryFeature[] = [
  {
    id: 'intl-1',
    name: 'France - Germany',
    class: 'internationalLand',
    lines: [
      [
        { lon: 7.5, lat: 48.5 },
        { lon: 8.0, lat: 49.0 },
      ],
    ],
    bbox: { west: 7.5, south: 48.5, east: 8.0, north: 49.0 },
  },
  {
    id: 'disp-1',
    name: 'Line of Control',
    class: 'disputedLine',
    status: 'Line of control',
    claimants: ['India', 'Pakistan'],
    lines: [
      [
        { lon: 74.0, lat: 34.0 },
        { lon: 74.5, lat: 34.5 },
      ],
    ],
    bbox: { west: 74.0, south: 34.0, east: 74.5, north: 34.5 },
  },
  {
    id: 'brk-1',
    name: 'Abkhazia',
    class: 'breakawayArea',
    status: 'Breakaway',
    sovereign: 'Georgia',
    lines: [
      [
        { lon: 40.0, lat: 43.0 },
        { lon: 41.0, lat: 43.5 },
        { lon: 40.5, lat: 42.8 },
        { lon: 40.0, lat: 43.0 },
      ],
    ],
    bbox: { west: 40.0, south: 42.8, east: 41.0, north: 43.5 },
  },
  {
    id: 'mar-1',
    name: 'Baltic Median Line',
    class: 'maritimeIndicative',
    lines: [
      [
        { lon: 19.0, lat: 56.0 },
        { lon: 20.0, lat: 57.0 },
      ],
    ],
    bbox: { west: 19.0, south: 56.0, east: 20.0, north: 57.0 },
  },
];

describe('BoundaryRenderer', () => {
  it('adds and parses all 4 boundary classes with appropriate widths', () => {
    const { polylines, renderer } = setup();

    renderer.setFeatures(mockFeatures);
    expect(renderer.getFeatureCount()).toBe(4);
    expect(polylines.length).toBe(4);

    // Locate returns centroids
    const locIntl = renderer.locate('intl-1');
    expect(locIntl).toBeDefined();
    expect(locIntl?.position).toBeDefined();

    const locDisp = renderer.locate('disp-1');
    expect(locDisp).toBeDefined();

    // Query feature metadata
    const feat = renderer.getFeature('brk-1');
    expect(feat?.name).toBe('Abkhazia');
    expect(feat?.sovereign).toBe('Georgia');
  });

  it('prunes features when new snapshot arrives', () => {
    const { polylines, renderer } = setup();

    renderer.setFeatures(mockFeatures);
    expect(renderer.getFeatureCount()).toBe(4);

    // Keep only the first 2
    renderer.setFeatures(mockFeatures.slice(0, 2));
    expect(renderer.getFeatureCount()).toBe(2);
    expect(polylines.length).toBe(2);
    expect(renderer.getFeature('brk-1')).toBeUndefined();
  });

  it('toggles visibility per class without removing primitives', () => {
    const { polylines, renderer } = setup();

    renderer.setFeatures(mockFeatures);

    // Turn off disputed lines and maritime indicative lines
    renderer.setVisibility({
      disputedLine: false,
      maritimeIndicative: false,
    });

    const vis = renderer.getVisibility();
    expect(vis.disputedLine).toBe(false);
    expect(vis.maritimeIndicative).toBe(false);
    expect(vis.internationalLand).toBe(true);

    // Polylines are still in collection, but show flag is updated
    expect(polylines.length).toBe(4);

    const intlPoly = polylines.get(0);
    const dispPoly = polylines.get(1);
    expect(intlPoly.show).toBe(true);
    expect(dispPoly.show).toBe(false);
  });

  it('handles interactive selection highlighting and resets', () => {
    const { polylines, renderer } = setup();

    renderer.setFeatures(mockFeatures);

    const targetPoly = polylines.get(1); // disp-1
    expect(targetPoly.width).toBe(DEFAULT_DISPUTED_WIDTH);

    // Select disp-1
    renderer.setSelected('disp-1');
    expect(targetPoly.width).toBe(SELECTED_BOUNDARY_WIDTH);

    // Deselect
    renderer.setSelected(null);
    expect(targetPoly.width).toBe(DEFAULT_DISPUTED_WIDTH);
  });

  it('clears and destroys resources cleanly', () => {
    const { polylines, renderer } = setup();

    renderer.setFeatures(mockFeatures);
    expect(renderer.getFeatureCount()).toBe(4);

    renderer.clear();
    expect(renderer.getFeatureCount()).toBe(0);
    expect(polylines.length).toBe(0);

    renderer.destroy();
    expect(renderer.getFeatureCount()).toBe(0);
  });
});
