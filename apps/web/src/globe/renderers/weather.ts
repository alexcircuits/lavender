/**
 * Weather imagery layers (radar reflectivity, global/regional cloud satellite IR)
 * and lightning strikes point flash renderer.
 *
 * Connects to NOAA nowCOAST WMS tile manifests through platform services,
 * manages Cesium ImageryLayers with GeographicTilingScheme, and renders
 * temporal lightning strikes with alpha decay fading.
 */
import { Cartesian3, Color, Rectangle } from '@cesium/core';
import {
  GeographicTilingScheme,
  ImageryLayer,
  type ImageryLayerCollection,
  type PointPrimitive,
  PointPrimitiveCollection,
  type PrimitiveCollection,
  UrlTemplateImageryProvider,
} from '@cesium/engine';

export const LIGHTNING_COLOR =
  Color.fromCssColorString('#ffeb3b') ?? new Color(1.0, 0.92, 0.23, 1.0);
export const LIGHTNING_PIXEL_SIZE = 4;
export const LIGHTNING_FADE_DURATION_MS = 1_800_000; // 30 minutes

export interface WeatherBounds {
  readonly west: number;
  readonly south: number;
  readonly east: number;
  readonly north: number;
}

export interface WeatherManifest {
  readonly product: string;
  readonly title: string;
  readonly coverage: string;
  readonly description: string;
  readonly source: string;
  readonly attribution: string;
  readonly bounds?: WeatherBounds | null | undefined;
  readonly times: readonly string[];
  readonly latest?: string | null | undefined;
  readonly time?: string | null | undefined;
  readonly tileSize?: number | undefined;
  readonly maxLevel?: number | undefined;
  readonly tilingScheme?: string | undefined;
  readonly tileTemplate?: string | null | undefined;
  readonly unavailable?: boolean | undefined;
}

export interface LightningStrike {
  readonly id?: string | undefined;
  readonly lat: number;
  readonly lon: number;
  readonly timestamp: number; // Milliseconds epoch
  readonly intensity?: number | undefined;
}

interface StrikeEntry {
  readonly strike: LightningStrike;
  readonly point: PointPrimitive;
}

export interface WeatherLayerOptions {
  readonly radar?: boolean | undefined;
  readonly satellite?: boolean | undefined;
  readonly lightning?: boolean | undefined;
}

export interface WeatherRendererCesiumOverrides {
  readonly PointPrimitiveCollection?: unknown | undefined;
  readonly UrlTemplateImageryProvider?: unknown | undefined;
  readonly ImageryLayer?: unknown | undefined;
  readonly GeographicTilingScheme?: unknown | undefined;
}

export interface WeatherRendererOptions {
  readonly cesium?: WeatherRendererCesiumOverrides | undefined;
}

export class WeatherRenderer {
  private readonly imageryLayers: ImageryLayerCollection;
  private readonly pointCollection: PointPrimitiveCollection;
  private readonly options: WeatherRendererOptions;

  private radarLayer: ImageryLayer | null = null;
  private satelliteLayer: ImageryLayer | null = null;
  private radarManifest: WeatherManifest | null = null;
  private satelliteManifest: WeatherManifest | null = null;
  private radarTimeIndex = -1;
  private satelliteTimeIndex = -1;

  private strikes: StrikeEntry[] = [];
  private readonly layersVisible: { radar: boolean; satellite: boolean; lightning: boolean } = {
    radar: true,
    satellite: true,
    lightning: true,
  };

  constructor(
    imageryLayers: ImageryLayerCollection,
    primitives: PrimitiveCollection,
    options: WeatherRendererOptions = {},
  ) {
    this.imageryLayers = imageryLayers;
    this.options = options;
    const PointCollectionClass = (options.cesium?.PointPrimitiveCollection ??
      PointPrimitiveCollection) as typeof PointPrimitiveCollection;
    this.pointCollection = new PointCollectionClass();
    primitives.add(this.pointCollection);
  }

  /**
   * Sets the radar imagery manifest and updates or creates the Cesium imagery layer.
   */
  public setRadarManifest(manifest: WeatherManifest | null): void {
    this.radarManifest = manifest;
    if (!manifest || manifest.unavailable || manifest.times.length === 0) {
      this.removeRadarLayer();
      return;
    }
    this.radarTimeIndex = manifest.times.length - 1;
    this.rebuildRadarLayer();
  }

  /**
   * Sets the satellite cloud imagery manifest and updates or creates the Cesium imagery layer.
   */
  public setSatelliteManifest(manifest: WeatherManifest | null): void {
    this.satelliteManifest = manifest;
    if (!manifest || manifest.unavailable || manifest.times.length === 0) {
      this.removeSatelliteLayer();
      return;
    }
    this.satelliteTimeIndex = manifest.times.length - 1;
    this.rebuildSatelliteLayer();
  }

  /**
   * Sets the radar observation frame time index.
   */
  public setRadarTimeIndex(index: number): void {
    if (!this.radarManifest || index < 0 || index >= this.radarManifest.times.length) {
      return;
    }
    this.radarTimeIndex = index;
    this.rebuildRadarLayer();
  }

  /**
   * Replaces the set of active lightning strikes and positions point primitives.
   */
  public setLightningStrikes(
    strikes: readonly LightningStrike[],
    nowMs: number = Date.now(),
  ): void {
    this.pointCollection.removeAll();
    this.strikes = [];

    for (const strike of strikes) {
      const age = nowMs - strike.timestamp;
      if (age >= LIGHTNING_FADE_DURATION_MS) {
        continue;
      }
      const alpha = Math.max(0.05, 1.0 - age / LIGHTNING_FADE_DURATION_MS);
      const position = Cartesian3.fromDegrees(strike.lon, strike.lat, 0);
      const color = LIGHTNING_COLOR.withAlpha(alpha);

      const point = this.pointCollection.add({
        position,
        color,
        pixelSize: LIGHTNING_PIXEL_SIZE,
        show: this.layersVisible.lightning,
      });

      this.strikes.push({ strike, point });
    }
  }

  /**
   * Advances elapsed time, updating lightning point fade alpha.
   */
  public tick(_elapsedSeconds: number, nowMs: number = Date.now()): void {
    const surviving: StrikeEntry[] = [];
    for (const entry of this.strikes) {
      const age = nowMs - entry.strike.timestamp;
      if (age >= LIGHTNING_FADE_DURATION_MS) {
        this.pointCollection.remove(entry.point);
      } else {
        const alpha = Math.max(0.05, 1.0 - age / LIGHTNING_FADE_DURATION_MS);
        entry.point.color = LIGHTNING_COLOR.withAlpha(alpha);
        surviving.push(entry);
      }
    }
    this.strikes = surviving;
  }

  /**
   * Configures visibility for the weather sub-layers.
   */
  public setLayers(options: WeatherLayerOptions): void {
    if (options.radar !== undefined) {
      this.layersVisible.radar = options.radar;
      if (this.radarLayer) {
        this.radarLayer.show = options.radar;
      }
    }
    if (options.satellite !== undefined) {
      this.layersVisible.satellite = options.satellite;
      if (this.satelliteLayer) {
        this.satelliteLayer.show = options.satellite;
      }
    }
    if (options.lightning !== undefined) {
      this.layersVisible.lightning = options.lightning;
      for (const entry of this.strikes) {
        entry.point.show = options.lightning;
      }
    }
  }

  /**
   * Returns current count of rendered lightning strikes.
   */
  public getParticleCount(): number {
    return this.strikes.length;
  }

  public clear(): void {
    this.removeRadarLayer();
    this.removeSatelliteLayer();
    this.pointCollection.removeAll();
    this.strikes = [];
    this.radarManifest = null;
    this.satelliteManifest = null;
  }

  public destroy(): void {
    this.clear();
    this.pointCollection.destroy();
  }

  private rebuildRadarLayer(): void {
    this.removeRadarLayer();
    if (!this.radarManifest || this.radarTimeIndex < 0) {
      return;
    }
    const time = this.radarManifest.times[this.radarTimeIndex];
    if (!time) {
      return;
    }

    const template =
      this.radarManifest.tileTemplate ??
      `/api/v1/tiles/weather/${this.radarManifest.product}/${encodeURIComponent(time)}/{z}/{x}/{y}.png`;

    const ProviderClass = (this.options.cesium?.UrlTemplateImageryProvider ??
      UrlTemplateImageryProvider) as typeof UrlTemplateImageryProvider;
    const SchemeClass = (this.options.cesium?.GeographicTilingScheme ??
      GeographicTilingScheme) as typeof GeographicTilingScheme;

    const providerOptions: {
      url: string;
      tilingScheme: GeographicTilingScheme;
      minimumLevel: number;
      maximumLevel: number;
      rectangle?: Rectangle;
    } = {
      url: template,
      tilingScheme: new SchemeClass(),
      minimumLevel: 0,
      maximumLevel: this.radarManifest.maxLevel ?? 6,
    };
    if (this.radarManifest.bounds) {
      providerOptions.rectangle = Rectangle.fromDegrees(
        this.radarManifest.bounds.west,
        this.radarManifest.bounds.south,
        this.radarManifest.bounds.east,
        this.radarManifest.bounds.north,
      );
    }

    const provider = new ProviderClass(providerOptions);

    const LayerClass = (this.options.cesium?.ImageryLayer ?? ImageryLayer) as typeof ImageryLayer;
    this.radarLayer = new LayerClass(provider);
    this.radarLayer.show = this.layersVisible.radar;
    this.radarLayer.alpha = 0.85;
    this.imageryLayers.add(this.radarLayer);
  }

  private rebuildSatelliteLayer(): void {
    this.removeSatelliteLayer();
    if (!this.satelliteManifest || this.satelliteTimeIndex < 0) {
      return;
    }
    const time = this.satelliteManifest.times[this.satelliteTimeIndex];
    if (!time) {
      return;
    }

    const template =
      this.satelliteManifest.tileTemplate ??
      `/api/v1/tiles/weather/${this.satelliteManifest.product}/${encodeURIComponent(time)}/{z}/{x}/{y}.png`;

    const ProviderClass = (this.options.cesium?.UrlTemplateImageryProvider ??
      UrlTemplateImageryProvider) as typeof UrlTemplateImageryProvider;
    const SchemeClass = (this.options.cesium?.GeographicTilingScheme ??
      GeographicTilingScheme) as typeof GeographicTilingScheme;

    const providerOptions: {
      url: string;
      tilingScheme: GeographicTilingScheme;
      minimumLevel: number;
      maximumLevel: number;
      rectangle?: Rectangle;
    } = {
      url: template,
      tilingScheme: new SchemeClass(),
      minimumLevel: 0,
      maximumLevel: this.satelliteManifest.maxLevel ?? 6,
    };
    if (this.satelliteManifest.bounds) {
      providerOptions.rectangle = Rectangle.fromDegrees(
        this.satelliteManifest.bounds.west,
        this.satelliteManifest.bounds.south,
        this.satelliteManifest.bounds.east,
        this.satelliteManifest.bounds.north,
      );
    }

    const provider = new ProviderClass(providerOptions);

    const LayerClass = (this.options.cesium?.ImageryLayer ?? ImageryLayer) as typeof ImageryLayer;
    this.satelliteLayer = new LayerClass(provider);
    this.satelliteLayer.show = this.layersVisible.satellite;
    this.satelliteLayer.alpha = 0.75;
    this.imageryLayers.add(this.satelliteLayer);
  }

  private removeRadarLayer(): void {
    if (this.radarLayer) {
      this.imageryLayers.remove(this.radarLayer, true);
      this.radarLayer = null;
    }
  }

  private removeSatelliteLayer(): void {
    if (this.satelliteLayer) {
      this.imageryLayers.remove(this.satelliteLayer, true);
      this.satelliteLayer = null;
    }
  }
}
