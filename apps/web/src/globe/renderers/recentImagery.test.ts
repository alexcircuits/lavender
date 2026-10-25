import { Cartesian3 } from '@cesium/core';
import type {
  ImageryLayer,
  ImageryLayerCollection,
  PrimitiveCollection,
  Scene,
} from '@cesium/engine';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { BBox } from '../../contracts';
import type { ImageryCandidate } from '../../platform/recentImageryTypes';
import { DEFAULT_PALETTE } from '../tokens';
import { buildGibsTileUrl, RecentImageryRenderer } from './recentImagery';
import { type RenderContext, toMapColors } from './types';

describe('RecentImageryRenderer', () => {
  let imageryLayers: {
    layers: ImageryLayer[];
    addImageryProvider: (provider: unknown) => ImageryLayer;
    remove: (layer: unknown, destroy?: boolean) => boolean;
  };
  let primitives: {
    collections: unknown[];
    add: <T>(c: T) => T;
    remove: (c: unknown) => boolean;
  };
  let scene: {
    canvas: HTMLCanvasElement;
    splitPosition: number;
    screenSpaceCameraController: { enableInputs: boolean };
    camera: {
      getPickRay: ReturnType<typeof vi.fn>;
      pickEllipsoid: ReturnType<typeof vi.fn>;
    };
    globe: {
      pick: ReturnType<typeof vi.fn>;
    };
  };
  let ctx: RenderContext;
  let renderer: RecentImageryRenderer;

  const mockCandidateA: ImageryCandidate = {
    key: 'S30-2026-10-20',
    product: 'S30',
    day: '2026-10-20',
    cloud: { min: 5, max: 15 },
    timeRange: null,
    availability: 'present',
    coverage: 'full',
    granuleCount: 2,
  };

  const mockCandidateB: ImageryCandidate = {
    key: 'L30-2026-10-18',
    product: 'L30',
    day: '2026-10-18',
    cloud: { min: 20, max: 30 },
    timeRange: null,
    availability: 'present',
    coverage: 'partial',
    granuleCount: 1,
  };

  const mockBox: BBox = {
    west: -97.8,
    south: 30.2,
    east: -97.7,
    north: 30.3,
  };

  beforeEach(() => {
    imageryLayers = {
      layers: [],
      addImageryProvider: (provider: unknown) => {
        const layer = {
          provider,
          alpha: 1,
          show: true,
          splitDirection: 0,
        } as unknown as ImageryLayer;
        imageryLayers.layers.push(layer);
        return layer;
      },
      remove: (layer: unknown) => {
        const idx = imageryLayers.layers.indexOf(layer as ImageryLayer);
        if (idx !== -1) {
          imageryLayers.layers.splice(idx, 1);
          return true;
        }
        return false;
      },
    };

    primitives = {
      collections: [],
      add: <T>(c: T) => {
        primitives.collections.push(c);
        return c;
      },
      remove: (c: unknown) => {
        const idx = primitives.collections.indexOf(c);
        if (idx !== -1) {
          primitives.collections.splice(idx, 1);
          return true;
        }
        return false;
      },
    };

    const canvas = document.createElement('canvas');
    canvas.getBoundingClientRect = () => ({
      left: 0,
      top: 0,
      width: 800,
      height: 600,
      right: 800,
      bottom: 600,
      x: 0,
      y: 0,
      toJSON: () => {},
    });

    scene = {
      canvas,
      splitPosition: 0.5,
      screenSpaceCameraController: { enableInputs: true },
      camera: {
        getPickRay: vi.fn(),
        pickEllipsoid: vi.fn(),
      },
      globe: {
        pick: vi.fn(),
      },
    };

    ctx = {
      colors: toMapColors(DEFAULT_PALETTE),
      requestRender: vi.fn(),
      cameraPosition: () => Cartesian3.ZERO,
      cameraHeightM: () => 1000,
    };

    renderer = new RecentImageryRenderer(
      imageryLayers as unknown as ImageryLayerCollection,
      primitives as unknown as PrimitiveCollection,
      scene as unknown as Scene,
      ctx,
    );
  });

  it('builds gibs tile URLs accurately according to product definitions', () => {
    const s30Url = buildGibsTileUrl('S30', '2026-10-20');
    expect(s30Url).toContain('HLS_S30_Nadir_BRDF_Adjusted_Reflectance');
    expect(s30Url).toContain('2026-10-20');
    expect(s30Url).toContain('GoogleMapsCompatible_Level12');
    expect(s30Url).toContain('.png');

    const viirsUrl = buildGibsTileUrl('VIIRS', '2026-10-20');
    expect(viirsUrl).toContain('VIIRS_SNPP_CorrectedReflectance_TrueColor');
    expect(viirsUrl).toContain('GoogleMapsCompatible_Level9');
    expect(viirsUrl).toContain('.jpg');
  });

  it('sets and clears bounding box preview without trailing render errors', () => {
    renderer.setBox(mockBox);
    expect(renderer.box).toEqual(mockBox);
    expect(ctx.requestRender).toHaveBeenCalled();

    renderer.setBox(null);
    expect(renderer.box).toBeNull();
  });

  it('mounts Slot A imagery and updates look based on mode', () => {
    renderer.setSlot('a', { candidate: mockCandidateA, box: mockBox });
    expect(renderer.slotA).not.toBeNull();
    expect(imageryLayers.layers.length).toBe(1);
    expect(renderer.slotA?.layer.alpha).toBe(1);
    expect(renderer.slotA?.layer.splitDirection).toBe(0); // SplitDirection.NONE in 'image' mode

    // Switch to basemap split mode
    renderer.setMode('basemap');
    expect(renderer.mode).toBe('basemap');
    expect(renderer.slotA?.layer.splitDirection).toBe(-1); // SplitDirection.LEFT
  });

  it('mounts Slot B imagery in A/B comparison mode', () => {
    renderer.setMode('ab');
    renderer.setSlot('a', { candidate: mockCandidateA, box: mockBox });
    renderer.setSlot('b', { candidate: mockCandidateB, box: mockBox });

    expect(imageryLayers.layers.length).toBe(2);
    expect(renderer.slotA?.layer.splitDirection).toBe(-1); // SplitDirection.LEFT
    expect(renderer.slotB?.layer.splitDirection).toBe(1); // SplitDirection.RIGHT

    // Opacity changes affect both layers
    renderer.setOpacity(0.65);
    expect(renderer.opacity).toBe(0.65);
    expect(renderer.slotA?.layer.alpha).toBe(0.65);
    expect(renderer.slotB?.layer.alpha).toBe(0.65);
  });

  it('updates splitPosition smoothly', () => {
    renderer.setSplitPosition(0.42);
    expect(scene.splitPosition).toBe(0.42);
    expect(ctx.requestRender).toHaveBeenCalled();
  });

  it('handles box drawing workflow and cancellation', () => {
    const onBox = vi.fn();
    const onCancel = vi.fn();

    renderer.startBoxDrawing(onBox, onCancel);
    expect(renderer.isDrawing()).toBe(true);

    // Cancel drawing
    renderer.cancelBoxDrawing();
    expect(renderer.isDrawing()).toBe(false);
  });

  it('cleans up layers and resets scene on destroy', () => {
    renderer.setSlot('a', { candidate: mockCandidateA, box: mockBox });
    expect(imageryLayers.layers.length).toBe(1);

    renderer.destroy();
    expect(imageryLayers.layers.length).toBe(0);
    expect(scene.splitPosition).toBe(0.5);
  });
});
