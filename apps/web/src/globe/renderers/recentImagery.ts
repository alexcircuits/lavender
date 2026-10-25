import { Cartesian2, Cartesian3, Cartographic, Math as CesiumMath, Rectangle } from '@cesium/core';
import {
  type ImageryLayer,
  type ImageryLayerCollection,
  Material,
  PolylineCollection,
  type PrimitiveCollection,
  type Scene,
  SplitDirection,
  UrlTemplateImageryProvider,
  WebMercatorTilingScheme,
} from '@cesium/engine';
import type { BBox } from '../../contracts';
import type { ImageryCandidate } from '../../platform/recentImageryTypes';
import type { RenderContext } from './types';

export type SplitMode = 'image' | 'basemap' | 'ab';

export interface ProductSpec {
  readonly label: string;
  readonly name: string;
  readonly gibsLayer: string;
  readonly maxLevel: number;
  readonly format: 'png' | 'jpg';
}

export const PRODUCTS: Record<string, ProductSpec> = {
  S30: {
    label: 'HLS S30',
    name: 'Sentinel-2',
    gibsLayer: 'HLS_S30_Nadir_BRDF_Adjusted_Reflectance',
    maxLevel: 12,
    format: 'png',
  },
  L30: {
    label: 'HLS L30',
    name: 'Landsat 8/9',
    gibsLayer: 'HLS_L30_Nadir_BRDF_Adjusted_Reflectance',
    maxLevel: 12,
    format: 'png',
  },
  VIIRS: {
    label: 'VIIRS',
    name: 'VIIRS TrueColor',
    gibsLayer: 'VIIRS_SNPP_CorrectedReflectance_TrueColor',
    maxLevel: 9,
    format: 'jpg',
  },
};

export function buildGibsTileUrl(product: string, day: string): string {
  const spec = PRODUCTS[product];
  if (!spec) {
    return `https://gibs-{s}.earthdata.nasa.gov/wmts/epsg3857/best/${product}/default/${day}/GoogleMapsCompatible_Level9/{z}/{y}/{x}.jpg`;
  }
  return `https://gibs-{s}.earthdata.nasa.gov/wmts/epsg3857/best/${spec.gibsLayer}/default/${day}/GoogleMapsCompatible_Level${spec.maxLevel}/{z}/{y}/{x}.${spec.format}`;
}

export interface ImagerySlotConfig {
  readonly candidate: ImageryCandidate;
  readonly box: BBox;
  readonly tileTemplate?: string | undefined;
}

interface ActiveSlot {
  readonly layer: ImageryLayer;
  readonly candidate: ImageryCandidate;
  readonly box: BBox;
}

/**
 * Recent Imagery renderer and interactive box drawing tool.
 * Manages draped GIBS satellite tiles (Slot A and Slot B), split comparisons,
 * and ground preview box geometry.
 */
export class RecentImageryRenderer {
  readonly #imageryLayers: ImageryLayerCollection;
  readonly #polylineCollection: PolylineCollection;
  readonly #scene: Scene;
  readonly #ctx: RenderContext;
  #mode: SplitMode = 'image';
  #opacity = 1.0;
  #box: BBox | null = null;
  #slotA: ActiveSlot | null = null;
  #slotB: ActiveSlot | null = null;

  // Box drawing tool state
  #isDrawing = false;
  #anchor: { lon: number; lat: number } | null = null;
  #onBoxCallback: ((bbox: BBox) => void) | null = null;
  #onCancelCallback: (() => void) | null = null;
  #pointerDownHandler: ((e: PointerEvent) => void) | null = null;
  #pointerMoveHandler: ((e: PointerEvent) => void) | null = null;
  #pointerUpHandler: ((e: PointerEvent) => void) | null = null;
  #keyHandler: ((e: KeyboardEvent) => void) | null = null;
  #savedCameraInputs: boolean | null = null;
  #destroyed = false;

  constructor(
    imageryLayers: ImageryLayerCollection,
    primitives: PrimitiveCollection,
    scene: Scene,
    ctx: RenderContext,
  ) {
    this.#imageryLayers = imageryLayers;
    this.#polylineCollection = primitives.add(new PolylineCollection());
    this.#scene = scene;
    this.#ctx = ctx;
  }

  get box(): BBox | null {
    return this.#box;
  }

  get mode(): SplitMode {
    return this.#mode;
  }

  get opacity(): number {
    return this.#opacity;
  }

  get slotA(): ActiveSlot | null {
    return this.#slotA;
  }

  get slotB(): ActiveSlot | null {
    return this.#slotB;
  }

  isDrawing(): boolean {
    return this.#isDrawing;
  }

  setBox(box: BBox | null): void {
    if (this.#destroyed) return;
    this.#box = box;
    this.#polylineCollection.removeAll();
    if (box) {
      const { west, south, east, north } = box;
      const positions = Cartesian3.fromDegreesArray([
        west,
        south,
        east,
        south,
        east,
        north,
        west,
        north,
        west,
        south,
      ]);
      this.#polylineCollection.add({
        positions,
        width: 2,
        material: Material.fromType('Color', {
          color: this.#ctx.colors.selection,
        }),
      });
    }
    this.#ctx.requestRender();
  }

  setSlot(slotId: 'a' | 'b', config: ImagerySlotConfig | null): void {
    if (this.#destroyed) return;
    const current = slotId === 'a' ? this.#slotA : this.#slotB;
    if (current) {
      try {
        this.#imageryLayers.remove(current.layer, true);
      } catch {
        // layer already removed
      }
      if (slotId === 'a') this.#slotA = null;
      else this.#slotB = null;
    }

    if (!config) {
      this.#ctx.requestRender();
      return;
    }

    const { candidate, box, tileTemplate } = config;
    const template = tileTemplate ?? buildGibsTileUrl(candidate.product, candidate.day);
    const spec = PRODUCTS[candidate.product];
    const maxLevel = spec?.maxLevel ?? 12;
    const isPng = spec?.format === 'png';

    const provider = new UrlTemplateImageryProvider({
      url: template,
      subdomains: ['a', 'b', 'c'],
      minimumLevel: 0,
      maximumLevel: maxLevel,
      tilingScheme: new WebMercatorTilingScheme(),
      rectangle: Rectangle.fromDegrees(box.west, box.south, box.east, box.north),
      hasAlphaChannel: isPng,
      credit: 'NASA GIBS / EOSDIS',
    });

    const layer = this.#imageryLayers.addImageryProvider(provider);
    layer.alpha = this.#opacity;
    this.#applySplitDirection(layer, slotId, this.#mode);

    const activeSlot: ActiveSlot = { layer, candidate, box };
    if (slotId === 'a') this.#slotA = activeSlot;
    else this.#slotB = activeSlot;

    this.#ctx.requestRender();
  }

  setMode(mode: SplitMode): void {
    if (this.#destroyed || this.#mode === mode) return;
    this.#mode = mode;
    if (this.#slotA) {
      this.#applySplitDirection(this.#slotA.layer, 'a', mode);
    }
    if (this.#slotB) {
      this.#applySplitDirection(this.#slotB.layer, 'b', mode);
    }
    this.#ctx.requestRender();
  }

  setOpacity(opacity: number): void {
    if (this.#destroyed) return;
    this.#opacity = Math.max(0, Math.min(1, opacity));
    if (this.#slotA) this.#slotA.layer.alpha = this.#opacity;
    if (this.#slotB) this.#slotB.layer.alpha = this.#opacity;
    this.#ctx.requestRender();
  }

  setSplitPosition(fraction: number): void {
    if (this.#destroyed) return;
    const clamped = Math.max(0, Math.min(1, fraction));
    if (this.#scene.splitPosition !== clamped) {
      this.#scene.splitPosition = clamped;
      this.#ctx.requestRender();
    }
  }

  #applySplitDirection(layer: ImageryLayer, slotId: 'a' | 'b', mode: SplitMode): void {
    if (mode === 'image') {
      layer.splitDirection = SplitDirection.NONE;
      if (slotId === 'b') layer.show = false;
      else layer.show = true;
    } else if (mode === 'basemap') {
      layer.splitDirection = slotId === 'a' ? SplitDirection.LEFT : SplitDirection.NONE;
      if (slotId === 'b') layer.show = false;
      else layer.show = true;
    } else if (mode === 'ab') {
      layer.show = true;
      layer.splitDirection = slotId === 'a' ? SplitDirection.LEFT : SplitDirection.RIGHT;
    }
  }

  startBoxDrawing(onBox: (bbox: BBox) => void, onCancel?: () => void): void {
    if (this.#destroyed || this.#isDrawing) return;
    this.#isDrawing = true;
    this.#onBoxCallback = onBox;
    this.#onCancelCallback = onCancel ?? null;
    this.#anchor = null;

    const canvas = this.#scene.canvas;
    if (!canvas) return;

    this.#pointerDownHandler = (e: PointerEvent) => {
      if (e.button !== 0) return;
      const pt = this.#pickDegrees(e.clientX, e.clientY);
      if (!pt) return;
      this.#anchor = pt;
      this.#disableCameraControls();
      e.stopPropagation();
    };

    this.#pointerMoveHandler = (e: PointerEvent) => {
      if (!this.#anchor) return;
      const pt = this.#pickDegrees(e.clientX, e.clientY);
      if (!pt) return;
      const west = Math.min(this.#anchor.lon, pt.lon);
      const east = Math.max(this.#anchor.lon, pt.lon);
      const south = Math.max(-85.0511, Math.min(this.#anchor.lat, pt.lat));
      const north = Math.min(85.0511, Math.max(this.#anchor.lat, pt.lat));
      this.setBox({ west, south, east, north });
    };

    this.#pointerUpHandler = (e: PointerEvent) => {
      if (!this.#anchor) return;
      const pt = this.#pickDegrees(e.clientX, e.clientY);
      const anchor = this.#anchor;
      this.#anchor = null;
      this.#restoreCameraControls();

      if (pt) {
        const west = Math.min(anchor.lon, pt.lon);
        const east = Math.max(anchor.lon, pt.lon);
        const south = Math.max(-85.0511, Math.min(anchor.lat, pt.lat));
        const north = Math.min(85.0511, Math.max(anchor.lat, pt.lat));
        if (east - west > 0.001 && north - south > 0.001) {
          const finalBox: BBox = { west, south, east, north };
          this.setBox(finalBox);
          const cb = this.#onBoxCallback;
          this.cancelBoxDrawing();
          cb?.(finalBox);
          return;
        }
      }
      this.cancelBoxDrawing();
    };

    this.#keyHandler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        const cancelCb = this.#onCancelCallback;
        this.cancelBoxDrawing();
        cancelCb?.();
      }
    };

    canvas.addEventListener('pointerdown', this.#pointerDownHandler);
    window.addEventListener('pointermove', this.#pointerMoveHandler);
    window.addEventListener('pointerup', this.#pointerUpHandler);
    window.addEventListener('keydown', this.#keyHandler);
  }

  cancelBoxDrawing(): void {
    this.#isDrawing = false;
    this.#anchor = null;
    this.#restoreCameraControls();

    const canvas = this.#scene.canvas;
    if (canvas && this.#pointerDownHandler) {
      canvas.removeEventListener('pointerdown', this.#pointerDownHandler);
    }
    if (this.#pointerMoveHandler) {
      window.removeEventListener('pointermove', this.#pointerMoveHandler);
    }
    if (this.#pointerUpHandler) {
      window.removeEventListener('pointerup', this.#pointerUpHandler);
    }
    if (this.#keyHandler) {
      window.removeEventListener('keydown', this.#keyHandler);
    }
    this.#pointerDownHandler = null;
    this.#pointerMoveHandler = null;
    this.#pointerUpHandler = null;
    this.#keyHandler = null;
    this.#onBoxCallback = null;
    this.#onCancelCallback = null;
  }

  #pickDegrees(clientX: number, clientY: number): { lon: number; lat: number } | null {
    const canvas = this.#scene.canvas;
    if (!canvas) return null;
    const rect = canvas.getBoundingClientRect();
    const x = clientX - rect.left;
    const y = clientY - rect.top;
    if (x < 0 || x > rect.width || y < 0 || y > rect.height) return null;

    const ray = this.#scene.camera?.getPickRay?.(new Cartesian2(x, y));
    if (!ray) return null;
    const cartesian =
      this.#scene.globe?.pick?.(ray, this.#scene) ??
      this.#scene.camera?.pickEllipsoid?.(new Cartesian2(x, y));
    if (!cartesian) return null;
    const cartographic = Cartographic.fromCartesian(cartesian);
    if (!cartographic) return null;
    return {
      lon: CesiumMath.toDegrees(cartographic.longitude),
      lat: CesiumMath.toDegrees(cartographic.latitude),
    };
  }

  #disableCameraControls(): void {
    const ctrl = this.#scene.screenSpaceCameraController;
    if (!ctrl) return;
    if (this.#savedCameraInputs === null) {
      this.#savedCameraInputs = ctrl.enableInputs;
    }
    ctrl.enableInputs = false;
  }

  #restoreCameraControls(): void {
    const ctrl = this.#scene.screenSpaceCameraController;
    if (!ctrl || this.#savedCameraInputs === null) return;
    ctrl.enableInputs = this.#savedCameraInputs;
    this.#savedCameraInputs = null;
  }

  destroy(): void {
    this.#destroyed = true;
    this.cancelBoxDrawing();
    if (this.#slotA) {
      try {
        this.#imageryLayers.remove(this.#slotA.layer, true);
      } catch {
        // ignore
      }
      this.#slotA = null;
    }
    if (this.#slotB) {
      try {
        this.#imageryLayers.remove(this.#slotB.layer, true);
      } catch {
        // ignore
      }
      this.#slotB = null;
    }
    this.#polylineCollection.removeAll();
    this.#scene.splitPosition = 0.5;
  }
}
