import { Cartographic, Event as CesiumEvent, Math as CesiumMath, Rectangle } from '@cesium/core';
import {
  ImageryLayer,
  ImageryLayerCollection,
  type Scene,
  UrlTemplateImageryProvider,
  WebMapTileServiceImageryProvider,
} from '@cesium/engine';
import { describe, expect, it, vi } from 'vitest';
import type { ImageryDescriptor } from '../contracts';
import { BaseImagery, BUNDLED_IMAGERY, createImageryProvider, gibsTilingScheme } from './imagery';
import { gibsMatrixSize, gibsTileSpanDeg, imageryKey, imageryProblem } from './imagerySources';

/** The catalog's default (crates/lavender-core/src/sources.rs). */
const GIBS_NIGHT: ImageryDescriptor = {
  id: 'gibs-black-marble',
  name: 'Night lights (VIIRS Black Marble 2016)',
  kind: 'wmtsGeographic',
  url: 'https://gibs.earthdata.nasa.gov/wmts/epsg4326/best/VIIRS_Black_Marble/default/2016-01-01/500m/{TileMatrix}/{TileRow}/{TileCol}.png',
  layer: 'VIIRS_Black_Marble',
  tileMatrixSet: '500m',
  format: 'image/png',
  maximumLevel: 7,
  sourceId: 'nasa-gibs',
  tileHosts: ['gibs.earthdata.nasa.gov'],
  isDark: true,
  isDefault: true,
};

/**
 * GIBS EPSG:4326 TileMatrix MatrixWidth × MatrixHeight per level, as published in the GIBS
 * WMTS GetCapabilities for the 250m/500m/1km/2km sets (512 px tiles, 0.5625°/px at level 0).
 */
const GIBS_MATRICES = [
  [2, 1],
  [3, 2],
  [5, 3],
  [10, 5],
  [20, 10],
  [40, 20],
  [80, 40],
  [160, 80],
  [320, 160],
];

describe('GIBS geographic grid', () => {
  it('reproduces the published tile matrix sizes', () => {
    GIBS_MATRICES.forEach(([width, height], level) => {
      expect(gibsMatrixSize(level)).toEqual({ width, height });
    });
    expect(gibsTileSpanDeg(0)).toBe(288);
  });

  it('gives Cesium 288° level-0 tiles anchored at (-180°, 90°), not 180° ones', () => {
    const scheme = gibsTilingScheme();
    const toDeg = (r: Rectangle) =>
      [r.west, r.south, r.east, r.north].map(
        (v) => Math.round(CesiumMath.toDegrees(v) * 1e6) / 1e6,
      );
    expect(toDeg(scheme.tileXYToRectangle(0, 0, 0))).toEqual([-180, -198, 108, 90]);
    expect(toDeg(scheme.tileXYToRectangle(1, 0, 0))).toEqual([108, -198, 396, 90]);
    expect(toDeg(scheme.tileXYToRectangle(4, 1, 2))).toEqual([108, -54, 180, 18]);
  });

  it('never addresses a tile outside the GIBS matrix for a point on Earth', () => {
    const scheme = gibsTilingScheme();
    const corners = [
      [-180, 90],
      [179.999, 90],
      [-180, -89.999],
      [179.999, -89.999],
      [0, 0],
    ] as const;
    GIBS_MATRICES.forEach(([width = 0, height = 0], level) => {
      for (const [lon, lat] of corners) {
        const xy = scheme.positionToTileXY(Cartographic.fromDegrees(lon, lat), level);
        expect(xy).toBeDefined();
        expect(xy?.x).toBeLessThan(width);
        expect(xy?.y).toBeLessThan(height);
      }
      // The far corner lands in the last column/row GIBS publishes.
      const far = scheme.positionToTileXY(Cartographic.fromDegrees(179.999, -89.999), level);
      expect([far?.x, far?.y]).toEqual([width - 1, height - 1]);
    });
  });

  it('builds a 512 px WMTS provider limited to the world rectangle', async () => {
    const provider = await createImageryProvider(GIBS_NIGHT);
    expect(provider.tileWidth).toBe(512);
    expect(provider.tileHeight).toBe(512);
    expect(provider.maximumLevel).toBe(7);
    expect(Rectangle.equalsEpsilon(provider.rectangle, Rectangle.MAX_VALUE, 1e-12)).toBe(true);
  });
});

describe('imagery descriptors', () => {
  it('accepts the catalog GIBS layer and the bundled layer', () => {
    expect(imageryProblem(GIBS_NIGHT)).toBeNull();
    expect(imageryProblem(BUNDLED_IMAGERY)).toBeNull();
  });

  it('only allows tiles from the descriptor’s tile hosts, over https', () => {
    expect(imageryProblem({ ...GIBS_NIGHT, tileHosts: ['example.org'] })).toMatch(/not one of/);
    expect(
      imageryProblem({ ...GIBS_NIGHT, url: GIBS_NIGHT.url.replace('https:', 'http:') }),
    ).toMatch(/https/);
    expect(imageryProblem({ ...GIBS_NIGHT, url: '/relative/{TileMatrix}' })).toMatch(/valid/);
  });

  it('requires the WMTS template placeholders and identifiers', () => {
    expect(
      imageryProblem({ ...GIBS_NIGHT, url: 'https://gibs.earthdata.nasa.gov/x/{TileMatrix}' }),
    ).toMatch(/TileRow/);
    expect(imageryProblem({ ...GIBS_NIGHT, layer: '' })).toMatch(/layer/);
  });

  it('accepts a URL template on an allowed host', () => {
    const template: ImageryDescriptor = {
      ...GIBS_NIGHT,
      kind: 'urlTemplate',
      url: 'https://tiles.example.org/{z}/{x}/{y}.png',
      tileHosts: ['tiles.example.org'],
    };
    expect(imageryProblem(template)).toBeNull();
  });

  it('rejects a disallowed descriptor before building a provider', async () => {
    await expect(createImageryProvider({ ...GIBS_NIGHT, tileHosts: [] })).rejects.toThrow(
      /tile hosts/,
    );
  });

  it('keys descriptors by what they show', () => {
    expect(imageryKey(BUNDLED_IMAGERY)).toBe(imageryKey({ ...BUNDLED_IMAGERY, id: 'other' }));
    expect(imageryKey(GIBS_NIGHT)).not.toBe(imageryKey(BUNDLED_IMAGERY));
  });
});

describe('BaseImagery swaps', () => {
  const TEMPLATE: ImageryDescriptor = {
    ...GIBS_NIGHT,
    id: 'template',
    kind: 'urlTemplate',
    url: 'https://tiles.example.org/{z}/{x}/{y}.png',
    tileHosts: ['tiles.example.org'],
  };

  /** Just enough of a Scene: real layer collection, scripted render/tile events. */
  function fakeScene() {
    const imageryLayers = new ImageryLayerCollection();
    imageryLayers.add(new ImageryLayer(new UrlTemplateImageryProvider({ url: TEMPLATE.url })));
    const globe = { tilesLoaded: false, tileLoadProgressEvent: new CesiumEvent() };
    const postRender = new CesiumEvent();
    const requestRender = vi.fn();
    const scene = { imageryLayers, globe, postRender, requestRender } as unknown as Scene;
    /** Renders frames with the tile queue empty. */
    const settle = () => {
      globe.tilesLoaded = true;
      postRender.raiseEvent();
      postRender.raiseEvent();
    };
    return { scene, imageryLayers, settle };
  }

  it('adds the new base layer above the old one and removes the old one once loaded', async () => {
    const { scene, imageryLayers, settle } = fakeScene();
    const original = imageryLayers.get(0);
    const imagery = new BaseImagery(scene, () => true);
    imagery.show(GIBS_NIGHT);
    await vi.waitFor(() => expect(imageryLayers.length).toBe(2));
    const incoming = imageryLayers.get(1);
    expect(incoming.alpha).toBe(0); // still hidden: no blank flash while it loads
    expect(imageryLayers.get(0)).toBe(original);
    settle();
    await vi.waitFor(() => expect(imageryLayers.length).toBe(1));
    expect(imageryLayers.get(0)).toBe(incoming);
    expect(incoming.alpha).toBe(1);
    expect(incoming.imageryProvider).toBeInstanceOf(WebMapTileServiceImageryProvider);
    imagery.destroy();
  });

  it('keeps the current layer and warns when a descriptor is not allowed', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const { scene, imageryLayers } = fakeScene();
    const original = imageryLayers.get(0);
    const imagery = new BaseImagery(scene, () => true);
    imagery.show({ ...GIBS_NIGHT, tileHosts: ['elsewhere.example'] });
    await vi.waitFor(() => expect(warn).toHaveBeenCalled());
    expect(warn.mock.calls[0]?.[0]).toMatch(/bundled offline imagery/);
    expect(imageryLayers.length).toBe(1);
    expect(imageryLayers.get(0)).toBe(original);
    imagery.destroy();
  });

  it('drops a provider whose first tiles fail, keeping what was shown', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const { scene, imageryLayers } = fakeScene();
    const original = imageryLayers.get(0);
    const imagery = new BaseImagery(scene, () => true);
    imagery.show(TEMPLATE);
    await vi.waitFor(() => expect(imageryLayers.length).toBe(2));
    const failing = imageryLayers.get(1).imageryProvider;
    failing.errorEvent.raiseEvent({ level: 0, message: 'offline' });
    await vi.waitFor(() => expect(imageryLayers.length).toBe(1));
    expect(imageryLayers.get(0)).toBe(original);
    expect(warn).toHaveBeenCalledWith(expect.stringMatching(/first tiles failed/));
    imagery.destroy();
  });

  it('returning to the shown imagery abandons the swap in flight', async () => {
    const { scene, imageryLayers } = fakeScene();
    const original = imageryLayers.get(0);
    const imagery = new BaseImagery(scene, () => true);
    imagery.show(GIBS_NIGHT);
    await vi.waitFor(() => expect(imageryLayers.length).toBe(2));
    imagery.show(BUNDLED_IMAGERY);
    expect(imageryLayers.length).toBe(1);
    expect(imageryLayers.get(0)).toBe(original);
    imagery.destroy();
  });

  it('stops a superseded fade instead of finishing it', async () => {
    const { scene, imageryLayers, settle } = fakeScene();
    const imagery = new BaseImagery(scene, () => false);
    imagery.show(GIBS_NIGHT);
    await vi.waitFor(() => expect(imageryLayers.length).toBe(2));
    const fading = imageryLayers.get(1);
    settle();
    await vi.waitFor(() => expect(fading.alpha).toBeGreaterThan(0));
    imagery.show(TEMPLATE);
    await vi.waitFor(() => expect(fading.isDestroyed()).toBe(true));
    expect(fading.alpha).toBeLessThan(1);
    imagery.destroy();
  });

  it('abandons a swap that a newer choice supersedes', async () => {
    const { scene, imageryLayers, settle } = fakeScene();
    const imagery = new BaseImagery(scene, () => true);
    imagery.show(GIBS_NIGHT);
    await vi.waitFor(() => expect(imageryLayers.length).toBe(2));
    imagery.show(TEMPLATE);
    await vi.waitFor(() =>
      expect(imageryLayers.get(imageryLayers.length - 1).imageryProvider).toBeInstanceOf(
        UrlTemplateImageryProvider,
      ),
    );
    settle();
    await vi.waitFor(() => expect(imageryLayers.length).toBe(1));
    expect(imageryLayers.get(0).imageryProvider).toBeInstanceOf(UrlTemplateImageryProvider);
    imagery.destroy();
  });
});
