/**
 * ALPR (automated license plate reader) camera renderer.
 * Displays community-mapped cameras as magenta markers, capped to the cameras nearest the
 * viewport center (ported from `legacy/src/layers/alpr/presentation.js` `viewRecords`).
 */
import { Cartesian3, Color } from '@cesium/core';
import {
  type PointPrimitive,
  PointPrimitiveCollection,
  type PrimitiveCollection,
} from '@cesium/engine';
import type { AlprCamera, BBox } from '../../contracts';
import { KeyedEntries } from './keyed';
import type { Located, RenderContext } from './types';

const ALPR_COLOR = '#ff5c8a';
const MARKER_SIZE = 8;
const MARKER_SELECTED_SIZE = 14;

/** Legacy `MAX_RENDERED` (`legacy/src/layers/alpr/policy.js`). */
export const ALPR_MAX_RENDERED = 1500;

function bboxContains(bbox: BBox, lat: number, lon: number): boolean {
  if (lat < bbox.south || lat > bbox.north) return false;
  return bbox.west <= bbox.east
    ? lon >= bbox.west && lon <= bbox.east
    : lon >= bbox.west || lon <= bbox.east;
}

/**
 * Keep cameras inside `bbox`; when more than `maxRendered` remain, keep the nearest to the
 * box center (longitude scaled by cos(latitude)).
 */
export function filterAndCapAlprCameras(
  cameras: readonly AlprCamera[],
  bbox: BBox,
  maxRendered: number = ALPR_MAX_RENDERED,
): AlprCamera[] {
  const visible = cameras.filter((c) => bboxContains(bbox, c.position.lat, c.position.lon));
  if (visible.length <= maxRendered) return visible;
  const lat = (bbox.south + bbox.north) / 2;
  const span = bbox.west <= bbox.east ? bbox.east - bbox.west : bbox.east - bbox.west + 360;
  let lon = bbox.west + span / 2;
  if (lon > 180) lon -= 360;
  const scale = Math.cos((lat * Math.PI) / 180);
  const distance = (c: AlprCamera): number => {
    let dLon = c.position.lon - lon;
    if (dLon > 180) dLon -= 360;
    else if (dLon < -180) dLon += 360;
    return (c.position.lat - lat) ** 2 + (dLon * scale) ** 2;
  };
  return visible
    .map((camera) => ({ camera, d: distance(camera) }))
    .sort((a, b) => a.d - b.d || a.camera.id.localeCompare(b.camera.id))
    .slice(0, Math.max(0, maxRendered))
    .map((item) => item.camera);
}

const COMPASS = ['N', 'NE', 'E', 'SE', 'S', 'SW', 'W', 'NW'] as const;

/** Human-readable facing, e.g. `"NE (45°)"`, or `undefined` when no valid bearing exists. */
export function alprDirectionLabel(directionDeg: number | undefined): string | undefined {
  if (directionDeg === undefined || !Number.isFinite(directionDeg)) return undefined;
  const deg = ((directionDeg % 360) + 360) % 360;
  const point = COMPASS[Math.round(deg / 45) % 8] ?? 'N';
  return `${point} (${Math.round(deg)}°)`;
}

export class AlprRenderer {
  readonly #ctx: RenderContext;
  readonly #primitives: PrimitiveCollection;
  readonly #points: PointPrimitiveCollection;
  readonly #cameras = new Map<string, AlprCamera>();
  #selectedId: string | null = null;
  readonly #entries: KeyedEntries<AlprCamera, PointPrimitive>;

  constructor(
    primitives: PrimitiveCollection,
    ctx: RenderContext,
    points?: PointPrimitiveCollection,
  ) {
    this.#ctx = ctx;
    this.#primitives = primitives;
    this.#points = points ?? primitives.add(new PointPrimitiveCollection());

    this.#entries = new KeyedEntries<AlprCamera, PointPrimitive>((c) => c.id, {
      create: (camera, id) => {
        this.#cameras.set(id, camera);
        return this.#points.add({
          position: Cartesian3.fromDegrees(camera.position.lon, camera.position.lat, 0),
          color: this.#colorFor(id),
          pixelSize: id === this.#selectedId ? MARKER_SELECTED_SIZE : MARKER_SIZE,
          id: { feed: 'alpr', id },
        });
      },
      update: (point, camera, id) => {
        this.#cameras.set(id, camera);
        point.position = Cartesian3.fromDegrees(camera.position.lon, camera.position.lat, 0);
        point.color = this.#colorFor(id);
        point.pixelSize = id === this.#selectedId ? MARKER_SELECTED_SIZE : MARKER_SIZE;
      },
      remove: (point, id) => {
        this.#cameras.delete(id);
        this.#points.remove(point);
      },
    });
  }

  #colorFor(id: string): Color {
    if (id === this.#selectedId) return this.#ctx.colors.selection;
    return Color.fromCssColorString(ALPR_COLOR) ?? Color.MAGENTA;
  }

  /** Replaces displayed cameras; pass the already filtered/capped set. */
  setCameras(cameras: readonly AlprCamera[]): void {
    this.#entries.apply(cameras);
    this.#ctx.requestRender();
  }

  getCamera(id: string): AlprCamera | undefined {
    return this.#cameras.get(id);
  }

  setSelected(id: string | null): void {
    if (this.#selectedId === id) return;
    this.#selectedId = id;
    for (const [cameraId, point] of this.#entries.entries()) {
      point.color = this.#colorFor(cameraId);
      point.pixelSize = cameraId === id ? MARKER_SELECTED_SIZE : MARKER_SIZE;
    }
    this.#ctx.requestRender();
  }

  locate(id: string): Located | undefined {
    const camera = this.#cameras.get(id);
    if (!camera) return undefined;
    return {
      position: Cartesian3.fromDegrees(camera.position.lon, camera.position.lat, 0),
      symbolPx: MARKER_SELECTED_SIZE,
    };
  }

  clear(): void {
    this.#entries.clear();
    this.#cameras.clear();
    this.#ctx.requestRender();
  }

  destroy(): void {
    this.#entries.clear();
    this.#cameras.clear();
    this.#primitives.remove(this.#points);
  }
}
