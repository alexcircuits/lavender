/**
 * Public camera (CCTV) viewshed and frustum renderer.
 * Computes 3D viewing pyramids and monitor plane projections using the Zig WASM micro-kernel.
 * Governed per ADR 0008 (Sensitive rating, disabled by default).
 */
import { Cartesian3, Color } from '@cesium/core';
import {
  type PointPrimitive,
  PointPrimitiveCollection,
  type Polyline,
  PolylineCollection,
  type PrimitiveCollection,
} from '@cesium/engine';
import type { CameraSource, CctvDetectionsResponse } from '../../contracts';
import type { EntityRef } from '../index';
import { KeyedEntries } from './keyed';
import type { Located, RenderContext } from './types';
import { computeCctvFrustumWasm } from './wasmCctv';

export const CCTV_CAMERA_COLOR =
  Color.fromCssColorString('#38bdf8') ?? new Color(0.22, 0.74, 0.97, 0.85);
export const CCTV_FRUSTUM_COLOR =
  Color.fromCssColorString('#6be8ff') ?? new Color(0.42, 0.91, 1.0, 0.75);

export interface CctvRenderOptions {
  readonly showAllFrustums?: boolean;
}

export class CctvRenderer {
  readonly #ctx: RenderContext;
  readonly #primitives: PrimitiveCollection;
  readonly #points: PointPrimitiveCollection;
  readonly #frustumLines: PolylineCollection;
  readonly #cameras = new Map<string, CameraSource>();
  #selectedId: string | null = null;
  #activeFrustum: Polyline[] = [];
  #detections: CctvDetectionsResponse | null = null;
  readonly #entries: KeyedEntries<CameraSource, PointPrimitive>;

  constructor(
    primitives: PrimitiveCollection,
    ctx: RenderContext,
    lines?: PolylineCollection,
    points?: PointPrimitiveCollection,
  ) {
    this.#ctx = ctx;
    this.#primitives = primitives;
    this.#points = points ?? primitives.add(new PointPrimitiveCollection());
    this.#frustumLines = lines ?? primitives.add(new PolylineCollection());

    this.#entries = new KeyedEntries<CameraSource, PointPrimitive>((c) => c.id, {
      create: (cam, id) => {
        this.#cameras.set(id, cam);
        const isSelected = id === this.#selectedId;
        const color = isSelected ? this.#ctx.colors.selection : CCTV_CAMERA_COLOR;
        const pos = Cartesian3.fromDegrees(
          cam.lon,
          cam.lat,
          (cam.groundElevationM ?? 0) + (cam.mountHeightM ?? 24),
        );

        return this.#points.add({
          position: pos,
          color,
          pixelSize: isSelected ? 12 : 7,
          id: { feed: 'cctv', id } satisfies EntityRef,
        });
      },
      update: (point, cam, id) => {
        this.#cameras.set(id, cam);
        const isSelected = id === this.#selectedId;
        point.position = Cartesian3.fromDegrees(
          cam.lon,
          cam.lat,
          (cam.groundElevationM ?? 0) + (cam.mountHeightM ?? 24),
        );
        point.color = isSelected ? this.#ctx.colors.selection : CCTV_CAMERA_COLOR;
        point.pixelSize = isSelected ? 12 : 7;
      },
      remove: (point, id) => {
        this.#cameras.delete(id);
        this.#points.remove(point);
      },
    });
  }

  setCameras(cameras: readonly CameraSource[]): void {
    this.#entries.apply(cameras);
    this.#updateFrustum();
    this.#ctx.requestRender();
  }

  setSelected(id: string | null): void {
    this.#selectedId = id;
    for (const [camId, point] of this.#entries.entries()) {
      const isSelected = camId === id;
      point.color = isSelected ? this.#ctx.colors.selection : CCTV_CAMERA_COLOR;
      point.pixelSize = isSelected ? 12 : 7;
    }
    this.#updateFrustum();
    this.#ctx.requestRender();
  }

  setDetections(detections: CctvDetectionsResponse | null): void {
    this.#detections = detections;
    this.#ctx.requestRender();
  }

  getDetections(): CctvDetectionsResponse | null {
    return this.#detections;
  }

  getCamera(id: string): CameraSource | undefined {
    return this.#cameras.get(id);
  }

  #updateFrustum(): void {
    // Clear previous frustum lines
    for (const line of this.#activeFrustum) {
      if (typeof this.#frustumLines.remove === 'function') {
        this.#frustumLines.remove(line);
      }
    }
    this.#activeFrustum = [];

    if (!this.#selectedId || typeof this.#frustumLines.add !== 'function') return;

    const cam = this.#cameras.get(this.#selectedId);
    if (!cam) return;

    // Use Zig WebAssembly micro-kernel to compute viewshed pyramid
    const g = computeCctvFrustumWasm({
      lat: cam.lat,
      lon: cam.lon,
      groundAltM: cam.groundElevationM ?? 0,
      mountHeightM: cam.mountHeightM ?? 24,
      headingDeg: cam.headingDeg ?? 0,
      pitchDeg: cam.pitchDeg ?? -15,
      fovDeg: cam.fovDeg ?? 70,
      rangeM: cam.rangeM ?? 600,
    });

    const mount = Cartesian3.fromDegrees(g.mount[0], g.mount[1], g.mount[2]);
    const tl = Cartesian3.fromDegrees(g.tl[0], g.tl[1], g.tl[2]);
    const tr = Cartesian3.fromDegrees(g.tr[0], g.tr[1], g.tr[2]);
    const br = Cartesian3.fromDegrees(g.br[0], g.br[1], g.br[2]);
    const bl = Cartesian3.fromDegrees(g.bl[0], g.bl[1], g.bl[2]);

    const color = CCTV_FRUSTUM_COLOR;

    // 4 Corner Rays
    const rays = [
      [mount, tl],
      [mount, tr],
      [mount, br],
      [mount, bl],
      // Far Cap Rectangle
      [tl, tr],
      [tr, br],
      [br, bl],
      [bl, tl],
    ];

    for (const positions of rays) {
      const line = this.#frustumLines.add({
        positions,
        width: 1.5,
        material: color,
      });
      this.#activeFrustum.push(line);
    }
  }

  locate(id: string): Located | undefined {
    const cam = this.#cameras.get(id);
    if (!cam) return undefined;
    return {
      position: Cartesian3.fromDegrees(
        cam.lon,
        cam.lat,
        (cam.groundElevationM ?? 0) + (cam.mountHeightM ?? 24),
      ),
      symbolPx: 32,
    };
  }

  clear(): void {
    this.#entries.clear();
    this.#cameras.clear();
    this.#selectedId = null;
    this.#detections = null;
    for (const line of this.#activeFrustum) {
      if (typeof this.#frustumLines.remove === 'function') {
        this.#frustumLines.remove(line);
      }
    }
    this.#activeFrustum = [];
    this.#ctx.requestRender();
  }

  destroy(): void {
    this.#entries.clear();
    this.#cameras.clear();
    this.#selectedId = null;
    this.#detections = null;
    for (const line of this.#activeFrustum) {
      if (typeof this.#frustumLines.remove === 'function') {
        this.#frustumLines.remove(line);
      }
    }
    this.#activeFrustum = [];
    if (typeof this.#primitives.remove === 'function') {
      this.#primitives.remove(this.#points);
      this.#primitives.remove(this.#frustumLines);
    }
  }
}
