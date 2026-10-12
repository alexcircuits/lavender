/**
 * Base imagery: builds providers from catalog descriptors and swaps the base layer without a
 * blank globe. The new layer is added above the current one at alpha 0, its visible tiles
 * load, it fades in (420 ms, `--motion-media`; instant under reduced motion), and only then
 * is the old layer removed. A provider that cannot be built, or whose first tiles fail, falls
 * back to the bundled Natural Earth II layer with a console warning.
 *
 * No Cesium ion, no other network: tiles only come from the descriptor's `tileHosts`
 * (checked by `imageryProblem`), over https.
 */
import { Rectangle } from '@cesium/core';
import {
  buildModuleUrl,
  GeographicTilingScheme,
  ImageryLayer,
  type ImageryProvider,
  type Scene,
  TileMapServiceImageryProvider,
  type TileProviderError,
  UrlTemplateImageryProvider,
  WebMapTileServiceImageryProvider,
} from '@cesium/engine';
import type { ImageryDescriptor } from '../contracts';
import { standardEasing } from './camera';
import { GIBS_GEOGRAPHIC_GRID, imageryKey, imageryProblem } from './imagerySources';

export const BUNDLED_IMAGERY: ImageryDescriptor = {
  id: 'natural-earth-ii',
  name: 'Natural Earth II (offline)',
  kind: 'bundled',
  url: '',
  maximumLevel: 2,
  sourceId: 'natural-earth',
  tileHosts: [],
  isDark: false,
  isDefault: false,
};

/** Imagery crossfade (`--motion-media`). */
export const IMAGERY_FADE_MS = 420;
/** A swap that has not settled by then fades in with what it has. */
const SETTLE_TIMEOUT_MS = 10_000;
/** Tile errors during a swap that count as a failed provider (any level-0/1 error does). */
const MAX_SWAP_ERRORS = 8;

export function bundledProvider(): Promise<TileMapServiceImageryProvider> {
  return TileMapServiceImageryProvider.fromUrl(buildModuleUrl('Assets/Textures/NaturalEarthII'));
}

/** Cesium's view of the GIBS EPSG:4326 grid (see imagerySources.ts). */
export function gibsTilingScheme(): GeographicTilingScheme {
  const g = GIBS_GEOGRAPHIC_GRID;
  return new GeographicTilingScheme({
    rectangle: Rectangle.fromDegrees(g.west, g.south, g.east, g.north),
    numberOfLevelZeroTilesX: g.levelZeroTilesX,
    numberOfLevelZeroTilesY: g.levelZeroTilesY,
  });
}

/** Builds the provider for a descriptor; rejects when it may not or cannot be shown. */
export async function createImageryProvider(
  descriptor: ImageryDescriptor,
): Promise<ImageryProvider> {
  const problem = imageryProblem(descriptor);
  if (problem) throw new Error(problem);
  switch (descriptor.kind) {
    case 'bundled':
      return bundledProvider();
    case 'wmtsGeographic':
      return new WebMapTileServiceImageryProvider({
        url: descriptor.url,
        layer: descriptor.layer ?? '',
        style: 'default',
        tileMatrixSetID: descriptor.tileMatrixSet ?? '',
        format: descriptor.format ?? 'image/jpeg',
        tilingScheme: gibsTilingScheme(),
        rectangle: Rectangle.fromDegrees(-180, -90, 180, 90),
        tileWidth: GIBS_GEOGRAPHIC_GRID.tilePx,
        tileHeight: GIBS_GEOGRAPHIC_GRID.tilePx,
        maximumLevel: descriptor.maximumLevel,
      });
    case 'urlTemplate':
      return new UrlTemplateImageryProvider({
        url: descriptor.url,
        maximumLevel: descriptor.maximumLevel,
      });
  }
}

interface ManagedLayer {
  readonly key: string;
  readonly layer: ImageryLayer;
  /** Stops listening to the provider's tile errors. */
  readonly release: () => void;
}

type Settled = 'ready' | 'failed' | 'cancelled';

const messageOf = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

export class BaseImagery {
  readonly #scene: Scene;
  readonly #reducedMotion: () => boolean;
  /** Base layers this manager owns, bottom first; the last settled one is `#current`. */
  readonly #managed = new Set<ManagedLayer>();
  #current: ManagedLayer | null = null;
  #targetKey: string;
  #token = 0;
  #cancel: (() => void) | null = null;
  #frame = 0;
  #destroyed = false;

  /** Adopts the widget's existing base layer (the bundled one created at mount). */
  constructor(scene: Scene, reducedMotion: () => boolean) {
    this.#scene = scene;
    this.#reducedMotion = reducedMotion;
    this.#targetKey = imageryKey(BUNDLED_IMAGERY);
    const base = scene.imageryLayers.length > 0 ? scene.imageryLayers.get(0) : undefined;
    if (base) {
      this.#current = { key: this.#targetKey, layer: base, release: () => {} };
      this.#managed.add(this.#current);
    }
  }

  show(descriptor: ImageryDescriptor): void {
    const key = imageryKey(descriptor);
    if (this.#destroyed || key === this.#targetKey) return;
    this.#targetKey = key;
    this.#token += 1;
    this.#cancel?.();
    if (this.#current?.key === key) {
      // Back to what is already shown: abandon any swap in flight.
      for (const other of [...this.#managed]) if (other !== this.#current) this.#drop(other);
      return;
    }
    void this.#swap(descriptor, this.#token);
  }

  destroy(): void {
    this.#destroyed = true;
    this.#cancel?.();
    cancelAnimationFrame(this.#frame);
    for (const managed of this.#managed) managed.release();
    this.#managed.clear();
    this.#current = null;
  }

  #stale(token: number): boolean {
    return this.#destroyed || token !== this.#token;
  }

  async #swap(descriptor: ImageryDescriptor, token: number): Promise<void> {
    let provider: ImageryProvider;
    try {
      provider = await createImageryProvider(descriptor);
    } catch (error) {
      if (!this.#stale(token)) this.#fallBack(descriptor, messageOf(error), token);
      return;
    }
    if (this.#stale(token)) return;

    const layers = this.#scene.imageryLayers;
    const layer = new ImageryLayer(provider, { alpha: 0 });
    const errors = { early: 0, total: 0, warned: false };
    let onError = () => {};
    const release = provider.errorEvent.addEventListener((error: TileProviderError) => {
      errors.total += 1;
      if (error.level <= 1) errors.early += 1;
      onError();
      if (this.#current?.layer === layer && !errors.warned) {
        errors.warned = true;
        console.warn(
          `Globe imagery "${descriptor.name}": some tiles failed to load`,
          error.message,
        );
      }
    });
    const managed: ManagedLayer = { key: imageryKey(descriptor), layer, release };
    const below = this.#current ? layers.indexOf(this.#current.layer) : -1;
    layers.add(layer, below >= 0 ? below + 1 : undefined);
    this.#managed.add(managed);

    const settled = await this.#settle(errors, (check) => {
      onError = check;
    });
    if (settled === 'failed' && !this.#stale(token)) {
      this.#drop(managed);
      this.#fallBack(descriptor, 'its first tiles failed to load', token);
      return;
    }
    if (settled === 'cancelled' || this.#stale(token)) {
      this.#drop(managed);
      return;
    }
    await this.#fadeIn(layer, this.#reducedMotion() ? 0 : IMAGERY_FADE_MS, token);
    if (this.#stale(token)) {
      this.#drop(managed);
      return;
    }
    for (const other of [...this.#managed]) if (other !== managed) this.#drop(other);
    this.#current = managed;
    this.#scene.requestRender();
  }

  #fallBack(descriptor: ImageryDescriptor, reason: string, token: number): void {
    console.warn(
      `Globe imagery "${descriptor.name}" is unavailable (${reason}); showing the bundled offline imagery.`,
    );
    if (descriptor.kind === 'bundled') return;
    this.#targetKey = imageryKey(BUNDLED_IMAGERY);
    if (this.#current?.key === this.#targetKey) return;
    void this.#swap(BUNDLED_IMAGERY, token);
  }

  #drop(managed: ManagedLayer): void {
    managed.release();
    this.#managed.delete(managed);
    if (this.#current === managed) this.#current = null;
    if (!this.#destroyed) {
      this.#scene.imageryLayers.remove(managed.layer, true);
      this.#scene.requestRender();
    }
  }

  /** Resolves once the visible tiles have loaded, the provider fails, or it is cancelled. */
  #settle(
    errors: { readonly early: number; readonly total: number },
    setErrorCheck: (check: () => void) => void,
  ): Promise<Settled> {
    const scene = this.#scene;
    return new Promise<Settled>((resolve) => {
      let frames = 0;
      let done = false;
      const finish = (result: Settled) => {
        if (done) return;
        done = true;
        offRender();
        offProgress();
        clearTimeout(timer);
        if (this.#cancel === cancel) this.#cancel = null;
        resolve(result);
      };
      const check = () => {
        if (errors.early > 0 || errors.total >= MAX_SWAP_ERRORS) finish('failed');
        else if (frames >= 2 && scene.globe.tilesLoaded) finish('ready');
      };
      const offRender = scene.postRender.addEventListener(() => {
        frames += 1;
        check();
        // Request render mode: keep frames coming until the queue has been seen empty.
        if (!done && frames < 2) scene.requestRender();
      });
      const offProgress = scene.globe.tileLoadProgressEvent.addEventListener(check);
      const timer = setTimeout(() => finish('ready'), SETTLE_TIMEOUT_MS);
      const cancel = () => finish('cancelled');
      this.#cancel = cancel;
      setErrorCheck(check);
      scene.requestRender();
    });
  }

  /** Fades a layer in; a superseded swap stops fading at once (it is dropped next). */
  #fadeIn(layer: ImageryLayer, durationMs: number, token: number): Promise<void> {
    if (durationMs <= 0) {
      layer.alpha = 1;
      this.#scene.requestRender();
      return Promise.resolve();
    }
    return new Promise<void>((resolve) => {
      const start = performance.now();
      const step = (now: number) => {
        if (this.#stale(token) || layer.isDestroyed()) {
          resolve();
          return;
        }
        const t = Math.min(1, Math.max(0, (now - start) / durationMs));
        layer.alpha = standardEasing(t);
        this.#scene.requestRender();
        if (t < 1) this.#frame = requestAnimationFrame(step);
        else resolve();
      };
      this.#frame = requestAnimationFrame(step);
    });
  }
}
