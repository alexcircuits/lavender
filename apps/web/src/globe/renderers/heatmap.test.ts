import type { ImageryLayer, ImageryLayerCollection } from '@cesium/engine';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { installRasterCanvas } from '../../test/rasterCanvas';
import {
  createColorRampLut,
  DEFAULT_HEATMAP_BOUNDS,
  DEFAULT_HEATMAP_HEIGHT,
  DEFAULT_HEATMAP_RAMP,
  DEFAULT_HEATMAP_WIDTH,
  type HeatmapContact,
  HeatmapRenderer,
  renderGridToRgba,
} from './heatmap';
import { loadLavenderWasm } from './wasmPropagation';

interface MockImageryLayer {
  provider: unknown;
  alpha: number;
  show: boolean;
  destroy?: () => void;
}

function createMockImageryLayers() {
  const layers: MockImageryLayer[] = [];
  return {
    layers,
    addImageryProvider: vi.fn((provider: unknown) => {
      const layer: MockImageryLayer = {
        provider,
        alpha: 1.0,
        show: true,
        destroy: vi.fn(),
      };
      layers.push(layer);
      return layer as unknown as ImageryLayer;
    }),
    add: vi.fn((layer: unknown) => {
      layers.push(layer as MockImageryLayer);
      return layer as unknown as ImageryLayer;
    }),
    remove: vi.fn((layer: unknown, destroy?: boolean) => {
      const idx = layers.indexOf(layer as MockImageryLayer);
      if (idx !== -1) {
        const removed = layers.splice(idx, 1)[0];
        if (destroy && removed?.destroy) {
          removed.destroy();
        }
        return true;
      }
      return false;
    }),
    get length() {
      return layers.length;
    },
  };
}

describe('HeatmapRenderer & Color Ramp (Dense Contact Heatmaps)', () => {
  let raster: ReturnType<typeof installRasterCanvas>;

  beforeAll(async () => {
    await loadLavenderWasm();
  });

  beforeEach(() => {
    raster = installRasterCanvas();
  });

  afterEach(() => {
    raster.restore();
  });

  describe('createColorRampLut', () => {
    it('generates a 256-entry RGBA LUT', () => {
      const lut = createColorRampLut();
      expect(lut).toBeInstanceOf(Uint8ClampedArray);
      expect(lut.length).toBe(256 * 4);
    });

    it('has zero alpha at t=0 (transparent base)', () => {
      const lut = createColorRampLut();
      // t=0 is index 0: [r, g, b, a]
      expect(lut[3]).toBe(0);
    });

    it('reaches maximum ramp color at t=1 (index 255)', () => {
      const lut = createColorRampLut();
      const lastStop = DEFAULT_HEATMAP_RAMP[DEFAULT_HEATMAP_RAMP.length - 1];
      expect(lastStop).toBeDefined();
      if (!lastStop) return;
      const baseIdx = 255 * 4;
      expect(lut[baseIdx]).toBe(lastStop.r);
      expect(lut[baseIdx + 1]).toBe(lastStop.g);
      expect(lut[baseIdx + 2]).toBe(lastStop.b);
      expect(lut[baseIdx + 3]).toBe(Math.round(lastStop.a * 255));
    });

    it('interpolates smoothly between custom stops', () => {
      const customRamp = [
        { t: 0.0, r: 0, g: 0, b: 0, a: 0.0 },
        { t: 1.0, r: 200, g: 100, b: 50, a: 1.0 },
      ];
      const lut = createColorRampLut(customRamp);
      // Midway at index 127/128 (~0.5)
      const mid = 128 * 4;
      expect(lut[mid]).toBeGreaterThan(90);
      expect(lut[mid]).toBeLessThan(110);
      expect(lut[mid + 1]).toBeGreaterThan(45);
      expect(lut[mid + 1]).toBeLessThan(55);
    });

    it('handles empty stops gracefully', () => {
      const lut = createColorRampLut([]);
      expect(lut.length).toBe(1024);
      expect(lut.every((v) => v === 0)).toBe(true);
    });
  });

  describe('renderGridToRgba', () => {
    const lut = createColorRampLut();

    it('returns all zeros for an empty grid', () => {
      const grid = {
        width: 2,
        height: 2,
        weightSum: new Float32Array(4),
        count: new Float32Array(4),
        binned: 0,
      };
      const pixels = renderGridToRgba(grid, lut);
      expect(pixels.length).toBe(16);
      expect(pixels.every((v) => v === 0)).toBe(true);
    });

    it('maps cells correctly under logarithmic scale', () => {
      // 2x2 grid: top-left has weight 10, top-right has weight 100
      const grid = {
        width: 2,
        height: 2,
        weightSum: new Float32Array([10, 100, 0, 0]),
        count: new Float32Array([1, 1, 0, 0]),
        binned: 2,
      };
      const pixels = renderGridToRgba(grid, lut, 'logarithmic');
      // Cell 0 (weight 10): t = ln(11) / ln(101) ≈ 2.398 / 4.615 ≈ 0.52
      const cell0Alpha = pixels[3] ?? 0;
      // Cell 1 (weight 100, max): t = 1.0
      const cell1Alpha = pixels[7] ?? 0;
      // Cell 2, 3 (weight 0): transparent
      expect(cell0Alpha).toBeGreaterThan(0);
      expect(cell1Alpha).toBeGreaterThan(cell0Alpha);
      expect(pixels[11]).toBe(0);
      expect(pixels[15]).toBe(0);
    });

    it('maps cells proportionally under linear scale', () => {
      const grid = {
        width: 2,
        height: 2,
        weightSum: new Float32Array([50, 100, 0, 0]),
        count: new Float32Array([1, 1, 0, 0]),
        binned: 2,
      };
      const pixels = renderGridToRgba(grid, lut, 'linear');
      const p3 = pixels[3] ?? 0;
      const p7 = pixels[7] ?? 0;
      expect(p3).toBeGreaterThan(0);
      expect(p7).toBeGreaterThan(p3);
      expect(pixels[11]).toBe(0);
    });
  });

  describe('HeatmapRenderer instance lifecycle', () => {
    it('initializes with default settings and creates an imagery layer', () => {
      const imageryLayers = createMockImageryLayers();
      const requestRender = vi.fn();
      const renderer = new HeatmapRenderer(imageryLayers as unknown as ImageryLayerCollection, {
        requestRender,
      });

      expect(renderer.width).toBe(DEFAULT_HEATMAP_WIDTH);
      expect(renderer.height).toBe(DEFAULT_HEATMAP_HEIGHT);
      expect(renderer.bounds).toEqual(DEFAULT_HEATMAP_BOUNDS);
      expect(renderer.opacity).toBe(1.0);
      expect(renderer.isVisible).toBe(true);
      expect(renderer.isDestroyed).toBe(false);
      expect(renderer.canvas.width).toBe(DEFAULT_HEATMAP_WIDTH);
      expect(renderer.canvas.height).toBe(DEFAULT_HEATMAP_HEIGHT);

      expect(imageryLayers.addImageryProvider).toHaveBeenCalledTimes(1);
      expect(renderer.layer).not.toBeNull();
      expect(renderer.provider).not.toBeNull();
    });

    it('initializes with custom dimensions and options', () => {
      const imageryLayers = createMockImageryLayers();
      const renderer = new HeatmapRenderer(imageryLayers as unknown as ImageryLayerCollection, {
        width: 256,
        height: 128,
        scale: 'linear',
        opacity: 0.75,
        show: false,
        bounds: { lonMin: -10, lonMax: 10, latMin: -5, latMax: 5 },
      });

      expect(renderer.width).toBe(256);
      expect(renderer.height).toBe(128);
      expect(renderer.scale).toBe('linear');
      expect(renderer.opacity).toBe(0.75);
      expect(renderer.isVisible).toBe(false);
      expect(renderer.bounds).toEqual({ lonMin: -10, lonMax: 10, latMin: -5, latMax: 5 });
    });

    it('bins contacts, updates the canvas texture, and requests render', () => {
      const imageryLayers = createMockImageryLayers();
      const requestRender = vi.fn();
      const renderer = new HeatmapRenderer(
        imageryLayers as unknown as ImageryLayerCollection,
        { requestRender },
        {
          width: 4,
          height: 4,
          bounds: { lonMin: -100, lonMax: 100, latMin: -50, latMax: 50 },
        },
      );

      // Contact near north-west: lat 40, lon -80 -> row 0, col 0
      const contacts: HeatmapContact[] = [
        { lat: 40, lon: -80, weight: 5 },
        { lat: -40, lon: 80, weight: 10 },
      ];

      renderer.updateContacts(contacts);

      expect(renderer.grid).not.toBeNull();
      expect(renderer.grid?.binned).toBe(2);
      expect(requestRender).toHaveBeenCalled();

      // Verify NW cell (row 0, col 0 -> cell 0) has weight 5
      expect(renderer.grid?.weightSum[0]).toBeCloseTo(5, 4);

      // Verify SE cell (row 3, col 3 -> cell 15) has weight 10
      expect(renderer.grid?.weightSum[15]).toBeCloseTo(10, 4);

      // Canvas pixels: cell 0 and cell 15 should have non-zero alpha
      const pixels = renderer.pixels;
      expect(pixels[3]).toBeGreaterThan(0); // NW alpha
      expect(pixels[15 * 4 + 3]).toBeGreaterThan(0); // SE alpha

      // Cell 1 (row 0, col 1) should remain transparent
      expect(pixels[7]).toBe(0);
    });

    it('handles contacts with default weight (1.0) and clamped negative weights', () => {
      const imageryLayers = createMockImageryLayers();
      const renderer = new HeatmapRenderer(imageryLayers as unknown as ImageryLayerCollection, {
        width: 2,
        height: 2,
        bounds: { lonMin: -10, lonMax: 10, latMin: -10, latMax: 10 },
      });

      const contacts: HeatmapContact[] = [
        { lat: 5, lon: -5 }, // NW, weight undefined -> defaults to 1.0
        { lat: -5, lon: 5, weight: -3 }, // SE, negative weight -> clamped to 0
      ];

      renderer.updateContacts(contacts);
      expect(renderer.grid?.weightSum[0]).toBeCloseTo(1.0, 4);
      expect(renderer.grid?.weightSum[3]).toBe(0);
    });

    it('handles empty contacts safely without error', () => {
      const imageryLayers = createMockImageryLayers();
      const renderer = new HeatmapRenderer(imageryLayers as unknown as ImageryLayerCollection, {
        width: 2,
        height: 2,
      });

      renderer.updateContacts([]);
      expect(renderer.grid?.binned).toBe(0);
      expect(renderer.pixels.every((v) => v === 0)).toBe(true);
    });

    it('clips and skips out-of-bounds contacts', () => {
      const imageryLayers = createMockImageryLayers();
      const renderer = new HeatmapRenderer(imageryLayers as unknown as ImageryLayerCollection, {
        width: 2,
        height: 2,
        bounds: { lonMin: -10, lonMax: 10, latMin: -10, latMax: 10 },
      });

      const contacts: HeatmapContact[] = [
        { lat: 0, lon: 0 }, // in bounds
        { lat: 50, lon: 0 }, // lat out of bounds
        { lat: 0, lon: -50 }, // lon out of bounds
        { lat: Number.NaN, lon: 0 }, // NaN
      ];

      renderer.updateContacts(contacts);
      expect(renderer.grid?.binned).toBe(1);
    });

    it('draws into raster context and allows pixel inspection', () => {
      const imageryLayers = createMockImageryLayers();
      const renderer = new HeatmapRenderer(imageryLayers as unknown as ImageryLayerCollection, {
        width: 4,
        height: 4,
        bounds: { lonMin: -10, lonMax: 10, latMin: -10, latMax: 10 },
      });

      // Add a single contact in NW cell (col 0, row 0)
      renderer.updateContacts([{ lat: 8, lon: -8, weight: 1 }]);

      const ctx = raster.contextOf(renderer.canvas);
      const nwPixel = ctx.pixel(0, 0);
      expect(nwPixel[3]).toBeGreaterThan(0); // non-zero alpha

      const sePixel = ctx.pixel(3, 3);
      expect(sePixel[3]).toBe(0); // clear
    });

    it('toggles visibility with show()', () => {
      const imageryLayers = createMockImageryLayers();
      const requestRender = vi.fn();
      const renderer = new HeatmapRenderer(imageryLayers as unknown as ImageryLayerCollection, {
        requestRender,
      });

      expect(renderer.isVisible).toBe(true);
      expect(renderer.layer?.show).toBe(true);

      renderer.show(false);
      expect(renderer.isVisible).toBe(false);
      expect(renderer.layer?.show).toBe(false);
      expect(requestRender).toHaveBeenCalled();

      renderer.show(true);
      expect(renderer.isVisible).toBe(true);
      expect(renderer.layer?.show).toBe(true);
    });

    it('adjusts opacity with setOpacity() and clamps values', () => {
      const imageryLayers = createMockImageryLayers();
      const requestRender = vi.fn();
      const renderer = new HeatmapRenderer(imageryLayers as unknown as ImageryLayerCollection, {
        requestRender,
      });

      renderer.setOpacity(0.42);
      expect(renderer.opacity).toBeCloseTo(0.42, 2);
      expect(renderer.layer?.alpha).toBeCloseTo(0.42, 2);
      expect(requestRender).toHaveBeenCalled();

      renderer.setOpacity(2.5); // over 1
      expect(renderer.opacity).toBe(1.0);

      renderer.setOpacity(-0.5); // under 0
      expect(renderer.opacity).toBe(0.0);
    });

    it('clears grid, pixels, and canvas with clear()', () => {
      const imageryLayers = createMockImageryLayers();
      const requestRender = vi.fn();
      const renderer = new HeatmapRenderer(
        imageryLayers as unknown as ImageryLayerCollection,
        { requestRender },
        { width: 4, height: 4 },
      );

      renderer.updateContacts([{ lat: 0, lon: 0, weight: 1 }]);
      expect(renderer.grid).not.toBeNull();
      expect(renderer.pixels.some((v) => v > 0)).toBe(true);

      renderer.clear();
      expect(renderer.grid).toBeNull();
      expect(renderer.pixels.every((v) => v === 0)).toBe(true);

      const ctx = raster.contextOf(renderer.canvas);
      expect(ctx.pixel(2, 2)[3]).toBe(0);
      expect(requestRender).toHaveBeenCalled();
    });

    it('destroys the renderer and detaches imagery layer', () => {
      const imageryLayers = createMockImageryLayers();
      const requestRender = vi.fn();
      const renderer = new HeatmapRenderer(imageryLayers as unknown as ImageryLayerCollection, {
        requestRender,
      });

      const initialLayer = renderer.layer;
      expect(imageryLayers.layers.length).toBe(1);

      renderer.destroy();
      expect(renderer.isDestroyed).toBe(true);
      expect(renderer.layer).toBeNull();
      expect(renderer.provider).toBeNull();
      expect(renderer.grid).toBeNull();
      expect(imageryLayers.remove).toHaveBeenCalledWith(initialLayer, true);
      expect(imageryLayers.layers.length).toBe(0);

      // Post-destruction calls are safe no-ops
      renderer.updateContacts([{ lat: 0, lon: 0 }]);
      renderer.show(true);
      renderer.setOpacity(0.5);
      renderer.clear();
      renderer.destroy();
      expect(renderer.isDestroyed).toBe(true);
    });

    it('exports data URL with toDataUrl()', () => {
      const imageryLayers = createMockImageryLayers();
      const renderer = new HeatmapRenderer(imageryLayers as unknown as ImageryLayerCollection);
      const url = renderer.toDataUrl();
      expect(typeof url).toBe('string');
    });
  });
});
