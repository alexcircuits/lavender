/**
 * Pointer interaction over the widget's single ScreenSpaceEventHandler: clicks pick an entity
 * (primitives carry an `EntityRef` as their `id`) and pointer moves report the geographic
 * position under the cursor, throttled to animation frames. Neither requests a render.
 */
import { Cartesian2, Cartesian3, Cartographic, Math as CesiumMath } from '@cesium/core';
import { type CesiumWidget, ScreenSpaceEventType } from '@cesium/engine';
import type { GeoPoint } from '../contracts';
import { entityFromPick } from './entityRef';
import type { EntityRef } from './index';

export interface InteractionHandlers {
  readonly pick: (entity: EntityRef | null) => void;
  readonly cursor: (position: GeoPoint | null) => void;
  readonly point?: (position: GeoPoint | null) => void;
  readonly drawing?: () => boolean;
}

const scratchCartesian = new Cartesian3();
const scratchCartographic = new Cartographic();

/** The ellipsoid point under a window position, in degrees, or null (sky, space). */
export function geoPointAt(widget: CesiumWidget, windowPosition: Cartesian2): GeoPoint | null {
  const scene = widget.scene;
  const hit = scene.camera.pickEllipsoid(windowPosition, scene.globe.ellipsoid, scratchCartesian);
  if (!hit) return null;
  const carto = Cartographic.fromCartesian(hit, scene.globe.ellipsoid, scratchCartographic);
  if (!carto) return null;
  return {
    lat: CesiumMath.toDegrees(carto.latitude),
    lon: CesiumMath.toDegrees(carto.longitude),
  };
}

export class Interaction {
  readonly #widget: CesiumWidget;
  readonly #handlers: InteractionHandlers;
  readonly #pointer = new Cartesian2();
  #pending = false;
  #frame = 0;
  #inside = false;
  #destroyed = false;

  constructor(widget: CesiumWidget, handlers: InteractionHandlers) {
    this.#widget = widget;
    this.#handlers = handlers;
    const events = widget.screenSpaceEventHandler;
    events.setInputAction(
      (click: { position: Cartesian2 }) => this.#pick(click.position),
      ScreenSpaceEventType.LEFT_CLICK,
    );
    events.setInputAction(
      (move: { endPosition: Cartesian2 }) => this.#move(move.endPosition),
      ScreenSpaceEventType.MOUSE_MOVE,
    );
    widget.canvas.addEventListener('pointerleave', this.#leave);
  }

  destroy(): void {
    if (this.#destroyed) return;
    this.#destroyed = true;
    cancelAnimationFrame(this.#frame);
    const events = this.#widget.screenSpaceEventHandler;
    if (!events.isDestroyed()) {
      events.removeInputAction(ScreenSpaceEventType.LEFT_CLICK);
      events.removeInputAction(ScreenSpaceEventType.MOUSE_MOVE);
    }
    this.#widget.canvas.removeEventListener('pointerleave', this.#leave);
  }

  #pick(position: Cartesian2): void {
    if (this.#handlers.drawing?.()) {
      this.#handlers.point?.(geoPointAt(this.#widget, position));
      return;
    }
    let entity: EntityRef | null = null;
    try {
      entity = entityFromPick(this.#widget.scene.pick(position));
    } catch (error) {
      console.error('Globe pick failed', error);
    }
    this.#handlers.pick(entity);
  }

  #move(position: Cartesian2): void {
    Cartesian2.clone(position, this.#pointer);
    this.#inside = true;
    if (this.#pending) return;
    this.#pending = true;
    this.#frame = requestAnimationFrame(() => {
      this.#pending = false;
      if (this.#destroyed || !this.#inside) return;
      this.#handlers.cursor(geoPointAt(this.#widget, this.#pointer));
    });
  }

  readonly #leave = (): void => {
    this.#inside = false;
    this.#handlers.cursor(null);
  };
}
