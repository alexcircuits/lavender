import type {
  ImageryLayer,
  ImageryLayerCollection,
  PointPrimitiveCollection,
  PrimitiveCollection,
  UrlTemplateImageryProvider,
} from '@cesium/engine';
import { describe, expect, it } from 'vitest';
import {
  LIGHTNING_FADE_DURATION_MS,
  type LightningStrike,
  type WeatherManifest,
  WeatherRenderer,
} from './weather';

function createMockHarness() {
  const imageryLayers: MockImageryLayer[] = [];
  const mockImageryLayersCollection = {
    add: (layer: unknown) => {
      imageryLayers.push(layer as MockImageryLayer);
      return layer;
    },
    remove: (layer: unknown) => {
      const idx = imageryLayers.indexOf(layer as MockImageryLayer);
      if (idx >= 0) {
        imageryLayers.splice(idx, 1);
        return true;
      }
      return false;
    },
    get length() {
      return imageryLayers.length;
    },
  };

  const primitives: unknown[] = [];
  const mockPrimitivesCollection = {
    add: (prim: unknown) => {
      primitives.push(prim);
      return prim;
    },
    remove: (prim: unknown) => {
      const idx = primitives.indexOf(prim);
      if (idx >= 0) {
        primitives.splice(idx, 1);
        return true;
      }
      return false;
    },
  };

  interface MockPoint {
    position: unknown;
    color: unknown;
    pixelSize: number;
    show: boolean;
  }

  class MockPointPrimitiveCollection {
    public points: MockPoint[] = [];
    public destroyed = false;
    public add(opts: MockPoint) {
      const pt = { ...opts, show: opts.show ?? true };
      this.points.push(pt);
      return pt;
    }
    public remove(pt: unknown) {
      const idx = this.points.indexOf(pt as MockPoint);
      if (idx >= 0) {
        this.points.splice(idx, 1);
        return true;
      }
      return false;
    }
    public removeAll() {
      this.points = [];
    }
    public destroy() {
      this.destroyed = true;
    }
  }

  class MockUrlTemplateImageryProvider {
    public url: string;
    public options: unknown;
    constructor(options: { url: string; [key: string]: unknown }) {
      this.url = options.url;
      this.options = options;
    }
  }

  class MockImageryLayer {
    public provider: unknown;
    public show = true;
    public alpha = 1.0;
    constructor(provider: unknown) {
      this.provider = provider;
    }
  }

  class MockGeographicTilingScheme {}

  const customCesium = {
    PointPrimitiveCollection:
      MockPointPrimitiveCollection as unknown as typeof PointPrimitiveCollection,
    UrlTemplateImageryProvider:
      MockUrlTemplateImageryProvider as unknown as typeof UrlTemplateImageryProvider,
    ImageryLayer: MockImageryLayer as unknown as typeof ImageryLayer,
    GeographicTilingScheme: MockGeographicTilingScheme as unknown,
  };

  const renderer = new WeatherRenderer(
    mockImageryLayersCollection as unknown as ImageryLayerCollection,
    mockPrimitivesCollection as unknown as PrimitiveCollection,
    { cesium: customCesium },
  );

  return {
    renderer,
    imageryLayers,
    primitives,
  };
}

describe('WeatherRenderer', () => {
  it('creates radar imagery layer from manifest and allows frame time switching', () => {
    const { renderer, imageryLayers } = createMockHarness();

    const manifest: WeatherManifest = {
      product: 'radar',
      title: 'CONUS Radar',
      coverage: 'CONUS',
      description: 'MRMS radar',
      source: 'NOAA nowCOAST',
      attribution: 'NOAA',
      times: ['2026-10-21T14:00:00Z', '2026-10-21T14:05:00Z'],
      bounds: { west: -130, south: 20, east: -60, north: 55 },
      tileTemplate: '/api/v1/tiles/weather/radar/{time}/{z}/{x}/{y}.png',
    };

    renderer.setRadarManifest(manifest);
    const layer = imageryLayers[0];
    expect(layer).toBeDefined();
    if (layer) {
      expect(layer.show).toBe(true);
      expect((layer.provider as { url: string }).url).toBe(
        '/api/v1/tiles/weather/radar/{time}/{z}/{x}/{y}.png',
      );
    }

    // Switch time index
    renderer.setRadarTimeIndex(0);
    expect(imageryLayers.length).toBe(1);

    // Null manifest removes layer
    renderer.setRadarManifest(null);
    expect(imageryLayers.length).toBe(0);
  });

  it('renders and temporal fades lightning strikes', () => {
    const { renderer } = createMockHarness();
    const now = 1_700_000_000_000;

    const strikes: LightningStrike[] = [
      { lat: 30.27, lon: -97.74, timestamp: now - 60_000 }, // 1 min old
      { lat: 30.5, lon: -97.5, timestamp: now - 1_200_000 }, // 20 min old
      {
        lat: 31.0,
        lon: -97.0,
        timestamp: now - LIGHTNING_FADE_DURATION_MS - 1000,
      }, // Expired
    ];

    renderer.setLightningStrikes(strikes, now);
    // The expired strike is excluded on placement
    expect(renderer.getParticleCount()).toBe(2);

    // Advance time: 20 min strike now reaches 31 mins old -> pruned
    renderer.tick(1, now + 700_000);
    expect(renderer.getParticleCount()).toBe(1);
  });

  it('handles layer visibility toggling and cleanup', () => {
    const { renderer, imageryLayers } = createMockHarness();

    const manifest: WeatherManifest = {
      product: 'clouds',
      title: 'Satellite IR',
      coverage: 'Global',
      description: 'GOES IR',
      source: 'NOAA nowCOAST',
      attribution: 'NOAA',
      times: ['2026-10-21T14:00:00Z'],
      tileTemplate: '/api/v1/tiles/weather/clouds/2026-10-21T14:00:00Z/{z}/{x}/{y}.png',
    };

    renderer.setSatelliteManifest(manifest);
    expect(imageryLayers.length).toBe(1);
    const layer = imageryLayers[0];
    expect(layer).toBeDefined();
    if (layer) {
      expect(layer.show).toBe(true);
    }

    renderer.setLayers({ satellite: false });
    if (layer) {
      expect(layer.show).toBe(false);
    }

    renderer.clear();
    expect(imageryLayers.length).toBe(0);
    expect(renderer.getParticleCount()).toBe(0);

    renderer.destroy();
  });
});
