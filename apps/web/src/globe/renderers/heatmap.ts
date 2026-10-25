/**
 * Dense Contact Heatmap Renderer (ADR 0009).
 *
 * Visualizes high-density geospatial contact distributions (aircraft, maritime
 * vessels, etc.) by binning contact points onto an equirectangular grid via the
 * Zig WebAssembly micro-kernel (`heatmapBin`), mapping density to an RGBA color
 * ramp, and draping the resulting offscreen texture over the globe using Cesium's
 * `SingleTileImageryProvider` / `ImageryLayer`.
 */
import { Rectangle } from '@cesium/core';
import {
  ImageryLayer,
  type ImageryLayerCollection,
  SingleTileImageryProvider,
} from '@cesium/engine';
import type { RenderContext } from './types';
import { type HeatmapBounds, type HeatmapGrid, heatmapBin } from './wasmHeatmap';

export interface HeatmapContact {
  readonly lat: number;
  readonly lon: number;
  readonly weight?: number;
}

export interface ColorStop {
  readonly t: number;
  readonly r: number;
  readonly g: number;
  readonly b: number;
  readonly a: number;
}

/**
 * High-visibility theme-friendly palette:
 * transparent blue -> cyan -> yellow -> orange -> magenta/red.
 */
export const DEFAULT_HEATMAP_RAMP: readonly ColorStop[] = [
  { t: 0.0, r: 0, g: 0, b: 0, a: 0.0 }, // transparent
  { t: 0.15, r: 0, g: 140, b: 255, a: 0.35 }, // transparent blue
  { t: 0.35, r: 0, g: 235, b: 220, a: 0.65 }, // cyan
  { t: 0.6, r: 255, g: 230, b: 30, a: 0.85 }, // yellow
  { t: 0.8, r: 255, g: 120, b: 0, a: 0.95 }, // orange
  { t: 1.0, r: 235, g: 20, b: 100, a: 1.0 }, // magenta / red
];

export const DEFAULT_HEATMAP_BOUNDS: HeatmapBounds = {
  lonMin: -180,
  lonMax: 180,
  latMin: -90,
  latMax: 90,
};

export const DEFAULT_HEATMAP_WIDTH = 512;
export const DEFAULT_HEATMAP_HEIGHT = 256;

export type DensityScale = 'logarithmic' | 'linear';

export interface HeatmapRendererOptions {
  readonly width?: number;
  readonly height?: number;
  readonly bounds?: HeatmapBounds;
  readonly scale?: DensityScale;
  readonly colorRamp?: readonly ColorStop[];
  readonly opacity?: number;
  readonly show?: boolean;
  readonly canvas?: HTMLCanvasElement;
}

/** 1x1 transparent PNG data URL fallback when canvas toDataURL is unavailable. */
const BLANK_PNG_DATA_URL =
  'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==';

/**
 * Precomputes a 256-entry RGBA lookup table (1024 bytes) by interpolating between color stops.
 */
export function createColorRampLut(
  ramp: readonly ColorStop[] = DEFAULT_HEATMAP_RAMP,
): Uint8ClampedArray {
  const lut = new Uint8ClampedArray(256 * 4);
  const sorted = [...ramp].sort((a, b) => a.t - b.t);
  if (sorted.length === 0) return lut;

  const first = sorted[0];
  const last = sorted[sorted.length - 1];
  if (!first || !last) return lut;

  for (let i = 0; i < 256; i++) {
    const t = i / 255;
    const baseIdx = i * 4;

    if (t <= first.t) {
      lut[baseIdx] = first.r;
      lut[baseIdx + 1] = first.g;
      lut[baseIdx + 2] = first.b;
      lut[baseIdx + 3] = Math.round(first.a * 255);
      continue;
    }

    if (t >= last.t) {
      lut[baseIdx] = last.r;
      lut[baseIdx + 1] = last.g;
      lut[baseIdx + 2] = last.b;
      lut[baseIdx + 3] = Math.round(last.a * 255);
      continue;
    }

    let j = 0;
    while (j < sorted.length - 1) {
      const next = sorted[j + 1];
      if (next && next.t < t) {
        j++;
      } else {
        break;
      }
    }

    const s0 = sorted[j] ?? first;
    const s1 = sorted[j + 1] ?? last;
    const span = s1.t - s0.t;
    const factor = span > 0 ? (t - s0.t) / span : 0;

    lut[baseIdx] = Math.round(s0.r + (s1.r - s0.r) * factor);
    lut[baseIdx + 1] = Math.round(s0.g + (s1.g - s0.g) * factor);
    lut[baseIdx + 2] = Math.round(s0.b + (s1.b - s0.b) * factor);
    lut[baseIdx + 3] = Math.round((s0.a + (s1.a - s0.a) * factor) * 255);
  }

  return lut;
}

/**
 * Maps a binned heatmap grid into RGBA pixels using the precomputed color ramp LUT.
 */
export function renderGridToRgba(
  grid: HeatmapGrid,
  lut: Uint8ClampedArray,
  scale: DensityScale = 'logarithmic',
  out?: Uint8ClampedArray,
): Uint8ClampedArray {
  const cells = grid.width * grid.height;
  const pixels = out ?? new Uint8ClampedArray(cells * 4);
  const weights = grid.weightSum;

  let maxVal = 0;
  for (let i = 0; i < cells; i++) {
    const w = weights[i] ?? 0;
    if (w > maxVal) maxVal = w;
  }

  if (maxVal <= 0) {
    pixels.fill(0);
    return pixels;
  }

  const logMax = Math.log1p(maxVal);

  for (let i = 0; i < cells; i++) {
    const val = weights[i] ?? 0;
    const pxIdx = i * 4;
    if (val <= 0) {
      pixels[pxIdx] = 0;
      pixels[pxIdx + 1] = 0;
      pixels[pxIdx + 2] = 0;
      pixels[pxIdx + 3] = 0;
      continue;
    }

    const t = scale === 'logarithmic' ? Math.log1p(val) / logMax : val / maxVal;
    const clampedT = Math.max(0, Math.min(1, t));
    const lutIdx = Math.min(255, Math.floor(clampedT * 255)) * 4;

    pixels[pxIdx] = lut[lutIdx] ?? 0;
    pixels[pxIdx + 1] = lut[lutIdx + 1] ?? 0;
    pixels[pxIdx + 2] = lut[lutIdx + 2] ?? 0;
    pixels[pxIdx + 3] = lut[lutIdx + 3] ?? 0;
  }

  return pixels;
}

export class HeatmapRenderer {
  readonly #imageryLayers: ImageryLayerCollection;
  readonly #ctx: { requestRender: () => void } | null = null;
  readonly #width: number;
  readonly #height: number;
  readonly #bounds: HeatmapBounds;
  readonly #scale: DensityScale;
  readonly #lut: Uint8ClampedArray;
  readonly #canvas: HTMLCanvasElement;
  #opacity: number;
  #show: boolean;
  #grid: HeatmapGrid | null = null;
  #pixels: Uint8ClampedArray;
  #layer: ImageryLayer | null = null;
  #provider: SingleTileImageryProvider | null = null;
  #destroyed = false;

  constructor(
    imageryLayers: ImageryLayerCollection,
    ctxOrOptions?: RenderContext | { requestRender: () => void } | HeatmapRendererOptions,
    options?: HeatmapRendererOptions,
  ) {
    this.#imageryLayers = imageryLayers;

    let opts: HeatmapRendererOptions | undefined;
    if (ctxOrOptions && 'requestRender' in ctxOrOptions) {
      this.#ctx = ctxOrOptions;
      opts = options;
    } else {
      opts = ctxOrOptions as HeatmapRendererOptions | undefined;
    }

    this.#width = opts?.width ?? DEFAULT_HEATMAP_WIDTH;
    this.#height = opts?.height ?? DEFAULT_HEATMAP_HEIGHT;
    this.#bounds = opts?.bounds ?? DEFAULT_HEATMAP_BOUNDS;
    this.#scale = opts?.scale ?? 'logarithmic';
    this.#opacity = Math.max(0, Math.min(1, opts?.opacity ?? 1.0));
    this.#show = opts?.show ?? true;
    this.#lut = createColorRampLut(opts?.colorRamp ?? DEFAULT_HEATMAP_RAMP);

    if (opts?.canvas) {
      this.#canvas = opts.canvas;
    } else if (typeof document !== 'undefined') {
      this.#canvas = document.createElement('canvas');
    } else {
      this.#canvas = {
        width: this.#width,
        height: this.#height,
        getContext: () => null,
        toDataURL: () => '',
      } as unknown as HTMLCanvasElement;
    }
    this.#canvas.width = this.#width;
    this.#canvas.height = this.#height;

    this.#pixels = new Uint8ClampedArray(this.#width * this.#height * 4);
    this.#updateImageryLayer();
  }

  get canvas(): HTMLCanvasElement {
    return this.#canvas;
  }

  get grid(): HeatmapGrid | null {
    return this.#grid;
  }

  get pixels(): Uint8ClampedArray {
    return this.#pixels;
  }

  get layer(): ImageryLayer | null {
    return this.#layer;
  }

  get provider(): SingleTileImageryProvider | null {
    return this.#provider;
  }

  get opacity(): number {
    return this.#opacity;
  }

  get isVisible(): boolean {
    return this.#show;
  }

  get isDestroyed(): boolean {
    return this.#destroyed;
  }

  get width(): number {
    return this.#width;
  }

  get height(): number {
    return this.#height;
  }

  get bounds(): HeatmapBounds {
    return this.#bounds;
  }

  get scale(): DensityScale {
    return this.#scale;
  }

  toDataUrl(): string {
    try {
      return typeof this.#canvas.toDataURL === 'function'
        ? this.#canvas.toDataURL('image/png')
        : '';
    } catch {
      return '';
    }
  }

  updateContacts(contacts: readonly HeatmapContact[]): void {
    if (this.#destroyed) return;

    const numContacts = contacts.length;
    const packed = new Float64Array(numContacts * 3);
    for (let i = 0; i < numContacts; i++) {
      const c = contacts[i];
      if (c) {
        packed[i * 3] = c.lon;
        packed[i * 3 + 1] = c.lat;
        packed[i * 3 + 2] = Math.max(0, c.weight ?? 1.0);
      }
    }

    const grid = heatmapBin(packed, this.#width, this.#height, this.#bounds);
    this.#grid = grid;

    renderGridToRgba(grid, this.#lut, this.#scale, this.#pixels);
    this.#drawToCanvas();
    this.#updateImageryLayer();
  }

  clear(): void {
    if (this.#destroyed) return;
    this.#grid = null;
    this.#pixels.fill(0);

    const ctx2d = this.#canvas.getContext?.('2d');
    if (ctx2d) {
      if (typeof ctx2d.clearRect === 'function') {
        ctx2d.clearRect(0, 0, this.#width, this.#height);
      } else if (typeof ctx2d.putImageData === 'function') {
        this.#drawToCanvas();
      }
    }

    this.#updateImageryLayer();
  }

  show(visible: boolean): void {
    if (this.#destroyed || this.#show === visible) return;
    this.#show = visible;
    if (this.#layer) {
      this.#layer.show = visible;
    }
    this.#ctx?.requestRender();
  }

  setOpacity(opacity: number): void {
    if (this.#destroyed) return;
    const clamped = Math.max(0, Math.min(1, opacity));
    if (this.#opacity === clamped) return;
    this.#opacity = clamped;
    if (this.#layer) {
      this.#layer.alpha = clamped;
    }
    this.#ctx?.requestRender();
  }

  destroy(): void {
    if (this.#destroyed) return;
    this.#destroyed = true;

    if (this.#layer && this.#imageryLayers && typeof this.#imageryLayers.remove === 'function') {
      try {
        this.#imageryLayers.remove(this.#layer, true);
      } catch {
        // layer already detached
      }
    }

    this.#layer = null;
    this.#provider = null;
    this.#grid = null;
    this.#ctx?.requestRender();
  }

  #drawToCanvas(): void {
    const ctx2d = this.#canvas.getContext?.('2d');
    if (!ctx2d) return;

    let imgData: ImageData | null = null;
    if (typeof ctx2d.createImageData === 'function') {
      imgData = ctx2d.createImageData(this.#width, this.#height);
    } else if (typeof ImageData !== 'undefined') {
      imgData = new ImageData(this.#width, this.#height);
    }

    if (imgData) {
      imgData.data.set(this.#pixels);
      if (typeof ctx2d.putImageData === 'function') {
        ctx2d.putImageData(imgData, 0, 0);
      }
    }
  }

  #updateImageryLayer(): void {
    if (this.#destroyed) return;

    const rawUrl = this.toDataUrl();
    const url = rawUrl && rawUrl.length > 25 ? rawUrl : BLANK_PNG_DATA_URL;

    const rectangle = Rectangle.fromDegrees(
      this.#bounds.lonMin,
      this.#bounds.latMin,
      this.#bounds.lonMax,
      this.#bounds.latMax,
    );

    const provider = new SingleTileImageryProvider({
      url,
      rectangle,
      tileWidth: this.#width,
      tileHeight: this.#height,
    });

    // Attach canvas to _image for direct zero-copy Cesium WebGL texture upload
    (provider as unknown as { _image: HTMLCanvasElement })._image = this.#canvas;

    const oldLayer = this.#layer;
    let newLayer: ImageryLayer | null = null;

    if (this.#imageryLayers) {
      if (typeof this.#imageryLayers.addImageryProvider === 'function') {
        newLayer = this.#imageryLayers.addImageryProvider(provider);
      } else if (typeof this.#imageryLayers.add === 'function') {
        newLayer = new ImageryLayer(provider);
        this.#imageryLayers.add(newLayer);
      }
    }

    if (newLayer) {
      newLayer.show = this.#show;
      newLayer.alpha = this.#opacity;
    }

    this.#layer = newLayer;
    this.#provider = provider;

    if (oldLayer && this.#imageryLayers && typeof this.#imageryLayers.remove === 'function') {
      try {
        this.#imageryLayers.remove(oldLayer, true);
      } catch {
        // layer already detached
      }
    }

    this.#ctx?.requestRender();
  }
}
