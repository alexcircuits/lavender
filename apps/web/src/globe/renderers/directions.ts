/**
 * Travel directions and turn-by-turn route renderer (OSRM).
 * Renders the road polyline (simplified via Zig WASM RDP micro-kernel),
 * origin/destination waypoint markers, and maneuver step dots.
 */
import { Cartesian3, Color } from '@cesium/core';
import {
  type PointPrimitive,
  PointPrimitiveCollection,
  type Polyline,
  PolylineCollection,
  type PrimitiveCollection,
} from '@cesium/engine';
import type { Route, RouteStep } from '../../contracts';
import type { EntityRef } from '../index';
import type { Located, RenderContext } from './types';
import { simplifyRdpWasm } from './wasmGeometry';

const DEFAULT_ROUTE_COLOR =
  Color.fromCssColorString('#38bdf8') ?? new Color(0.22, 0.74, 0.97, 0.95);
const ORIGIN_COLOR = Color.fromCssColorString('#22c55e') ?? new Color(0.13, 0.77, 0.37, 1.0);
const DESTINATION_COLOR = Color.fromCssColorString('#ef4444') ?? new Color(0.94, 0.27, 0.27, 1.0);
const STEP_DOT_COLOR = Color.fromCssColorString('#bae6fd') ?? new Color(0.73, 0.9, 0.99, 0.9);
const STEP_SELECTED_COLOR = Color.fromCssColorString('#f59e0b') ?? new Color(0.96, 0.62, 0.04, 1.0);

export class DirectionsRenderer {
  readonly #ctx: RenderContext;
  readonly #primitives: PrimitiveCollection;
  readonly #lines: PolylineCollection;
  readonly #points: PointPrimitiveCollection;

  #route: Route | null = null;
  #routePolyline: Polyline | null = null;
  #origin: [number, number] | null = null;
  #destination: [number, number] | null = null;
  #originPoint: PointPrimitive | null = null;
  #destinationPoint: PointPrimitive | null = null;
  #stepPoints: PointPrimitive[] = [];
  #selectedStepIndex: number | null = null;

  constructor(
    primitives: PrimitiveCollection,
    ctx: RenderContext,
    lines?: PolylineCollection,
    points?: PointPrimitiveCollection,
  ) {
    this.#ctx = ctx;
    this.#primitives = primitives;
    this.#lines = lines ?? primitives.add(new PolylineCollection());
    this.#points = points ?? primitives.add(new PointPrimitiveCollection());
  }

  getRoute(): Route | null {
    return this.#route;
  }

  getSelectedStep(): number | null {
    return this.#selectedStepIndex;
  }

  setWaypoints(origin: [number, number] | null, destination: [number, number] | null): void {
    this.#origin = origin;
    this.#destination = destination;

    if (this.#originPoint && typeof this.#points.remove === 'function') {
      this.#points.remove(this.#originPoint);
      this.#originPoint = null;
    }
    if (this.#destinationPoint && typeof this.#points.remove === 'function') {
      this.#points.remove(this.#destinationPoint);
      this.#destinationPoint = null;
    }

    if (origin && typeof this.#points.add === 'function') {
      this.#originPoint = this.#points.add({
        position: Cartesian3.fromDegrees(origin[0], origin[1], 0),
        color: ORIGIN_COLOR,
        pixelSize: 14,
        id: { feed: 'directions-waypoint', id: 'origin' } as unknown as EntityRef,
      });
    }

    if (destination && typeof this.#points.add === 'function') {
      this.#destinationPoint = this.#points.add({
        position: Cartesian3.fromDegrees(destination[0], destination[1], 0),
        color: DESTINATION_COLOR,
        pixelSize: 14,
        id: { feed: 'directions-waypoint', id: 'destination' } as unknown as EntityRef,
      });
    }

    this.#ctx.requestRender();
  }

  setRoute(route: Route | null): void {
    this.#route = route;

    // Clear old polyline
    if (this.#routePolyline && typeof this.#lines.remove === 'function') {
      this.#lines.remove(this.#routePolyline);
      this.#routePolyline = null;
    }

    // Clear old step dots
    for (const p of this.#stepPoints) {
      if (typeof this.#points.remove === 'function') {
        this.#points.remove(p);
      }
    }
    this.#stepPoints = [];
    this.#selectedStepIndex = null;

    if (!route?.ok || route.geometry.length < 2) {
      this.#ctx.requestRender();
      return;
    }

    // Simplify geometry with Zig WASM RDP kernel if dense (> 150 points)
    const rawCoords = route.geometry;
    let coordsToRender: readonly [number, number][] = rawCoords;
    if (rawCoords.length > 150) {
      const simplified = simplifyRdpWasm(rawCoords, 0.00005);
      if (simplified.length >= 2) {
        coordsToRender = simplified;
      }
    }

    const positions = coordsToRender.map(([lon, lat]) => Cartesian3.fromDegrees(lon, lat, 0));

    if (typeof this.#lines.add === 'function') {
      this.#routePolyline = this.#lines.add({
        positions,
        width: 3.5,
        material: DEFAULT_ROUTE_COLOR,
      });
    }

    // Render step maneuver markers
    if (route.steps && route.steps.length > 0 && typeof this.#points.add === 'function') {
      for (const step of route.steps) {
        const pt = this.#points.add({
          position: Cartesian3.fromDegrees(step.lon, step.lat, 0),
          color: STEP_DOT_COLOR,
          pixelSize: 7,
          id: { feed: 'directions-step', id: String(step.index) } as unknown as EntityRef,
        });
        this.#stepPoints.push(pt);
      }
    }

    this.#ctx.requestRender();
  }

  setSelectedStep(index: number | null): void {
    this.#selectedStepIndex = index;
    if (!this.#route?.steps) return;

    for (let i = 0; i < this.#stepPoints.length; i++) {
      const pt = this.#stepPoints[i];
      if (!pt) continue;
      const isSelected = i === index;
      pt.color = isSelected ? STEP_SELECTED_COLOR : STEP_DOT_COLOR;
      pt.pixelSize = isSelected ? 12 : 7;
    }

    this.#ctx.requestRender();
  }

  locateStep(index: number): Located | null {
    if (!this.#route?.steps) return null;
    const step: RouteStep | undefined = this.#route.steps.find((s) => s.index === index);
    if (!step) return null;

    return {
      position: Cartesian3.fromDegrees(step.lon, step.lat, 0),
      symbolPx: 12,
    };
  }

  locateWaypoint(kind: 'origin' | 'destination'): Located | null {
    const coords = kind === 'origin' ? this.#origin : this.#destination;
    if (!coords) return null;

    return {
      position: Cartesian3.fromDegrees(coords[0], coords[1], 0),
      symbolPx: 14,
    };
  }

  clear(): void {
    this.setRoute(null);
    this.setWaypoints(null, null);
  }

  destroy(): void {
    if (this.#originPoint && typeof this.#points.remove === 'function') {
      this.#points.remove(this.#originPoint);
      this.#originPoint = null;
    }
    if (this.#destinationPoint && typeof this.#points.remove === 'function') {
      this.#points.remove(this.#destinationPoint);
      this.#destinationPoint = null;
    }
    this.#origin = null;
    this.#destination = null;

    if (this.#routePolyline && typeof this.#lines.remove === 'function') {
      this.#lines.remove(this.#routePolyline);
      this.#routePolyline = null;
    }
    for (const p of this.#stepPoints) {
      if (typeof this.#points.remove === 'function') {
        this.#points.remove(p);
      }
    }
    this.#stepPoints = [];
    this.#selectedStepIndex = null;
    this.#route = null;

    if (typeof this.#primitives.remove === 'function') {
      this.#primitives.remove(this.#lines);
      this.#primitives.remove(this.#points);
    }
  }
}
