/**
 * Engine-free rules for base imagery descriptors: where tiles may come from (ADR 0008: only
 * the descriptor's `tileHosts`, over https) and the geometry of NASA GIBS's geographic
 * (EPSG:4326) tile matrix sets.
 *
 * GIBS EPSG:4326 grid (GIBS API documentation, GetCapabilities TileMatrix entries): 512 px
 * tiles from a top-left corner at (-180°, 90°); level 0 is 0.5625°/px, so one level-0 tile
 * spans 288° and the world needs a 2 × 1 matrix whose tiles run past 180°E and 90°S (the
 * excess is empty). Each level halves the tile span: matrices are 2×1, 3×2, 5×3, 10×5,
 * 20×10, … Cesium's default GeographicTilingScheme (2 × 1 tiles of 180°) would stretch these
 * tiles across the wrong area, so the engine uses a scheme with the same 288° level-0 tiles
 * (`GIBS_GEOGRAPHIC_GRID`), restricted to the world rectangle so it never asks for the empty
 * tiles beyond it.
 */
import type { ImageryDescriptor } from '../contracts';

export const GIBS_TILE_PX = 512;
export const GIBS_LEVEL0_DEG_PER_PX = 0.5625;
export const GIBS_LEVEL0_TILE_DEG = GIBS_TILE_PX * GIBS_LEVEL0_DEG_PER_PX; // 288°

/** The GIBS geographic tiling grid as a Cesium tiling-scheme rectangle, in degrees. */
export const GIBS_GEOGRAPHIC_GRID = {
  west: -180,
  north: 90,
  east: -180 + 2 * GIBS_LEVEL0_TILE_DEG,
  south: 90 - GIBS_LEVEL0_TILE_DEG,
  levelZeroTilesX: 2,
  levelZeroTilesY: 1,
  tilePx: GIBS_TILE_PX,
} as const;

export function gibsTileSpanDeg(level: number): number {
  return GIBS_LEVEL0_TILE_DEG / 2 ** level;
}

/** GIBS's TileMatrix width × height at a level (tiles needed to cover the world). */
export function gibsMatrixSize(level: number): { readonly width: number; readonly height: number } {
  const span = gibsTileSpanDeg(level);
  return { width: Math.ceil(360 / span), height: Math.ceil(180 / span) };
}

const PLACEHOLDER = /\{[^}]+\}/g;
const WMTS_PLACEHOLDERS = ['{TileMatrix}', '{TileRow}', '{TileCol}'] as const;

/**
 * Why a descriptor's tiles may not be requested, or null when they may. Bundled imagery is
 * always allowed: it is served from the app's own origin.
 */
export function imageryProblem(descriptor: ImageryDescriptor): string | null {
  if (descriptor.kind === 'bundled') return null;
  let url: URL;
  try {
    url = new URL(descriptor.url.replace(PLACEHOLDER, '0'));
  } catch {
    return 'the tile URL is not a valid absolute URL';
  }
  if (url.protocol !== 'https:') return 'tiles must be fetched over https';
  const allowed = descriptor.tileHosts.map((host) => host.trim().toLowerCase());
  if (!allowed.includes(url.hostname.toLowerCase())) {
    return `${url.hostname} is not one of the descriptor's tile hosts`;
  }
  if (descriptor.kind === 'wmtsGeographic') {
    const missing = WMTS_PLACEHOLDERS.filter((p) => !descriptor.url.includes(p));
    if (missing.length > 0) return `the WMTS template lacks ${missing.join(', ')}`;
    if (!descriptor.layer || !descriptor.tileMatrixSet) {
      return 'a WMTS layer needs `layer` and `tileMatrixSet`';
    }
  }
  if (!Number.isInteger(descriptor.maximumLevel) || descriptor.maximumLevel < 0) {
    return 'maximumLevel must be a non-negative integer';
  }
  return null;
}

/** A stable key: two descriptors with the same key show the same tiles. */
export function imageryKey(descriptor: ImageryDescriptor): string {
  return descriptor.kind === 'bundled' ? 'bundled' : `${descriptor.kind}:${descriptor.url}`;
}
