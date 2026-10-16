/** Bounded ellipsoid arcs, refusing unsupported antipodes without throwing into the globe. */
import { Cartesian3, Cartographic, EllipsoidGeodesic } from '@cesium/core';

export interface ArcBudget {
  remaining: number;
}
type Coordinate = readonly [number, number];

export function surfaceArcPositions(
  coordinates: readonly Coordinate[],
  budget: ArcBudget,
): Cartesian3[] | null {
  if (coordinates.length < 2) return null;
  const positions: Cartesian3[] = [];
  let remaining = budget.remaining;
  let lastLift = 50;
  try {
    for (let i = 1; i < coordinates.length; i++) {
      const a = coordinates[i - 1];
      const b = coordinates[i];
      if (
        !a ||
        !b ||
        [...a, ...b].some((v) => !Number.isFinite(v)) ||
        Math.abs(a[1]) > 90 ||
        Math.abs(b[1]) > 90
      )
        return null;
      const from = Cartesian3.fromDegrees(...a);
      const to = Cartesian3.fromDegrees(...b);
      const angle = Cartesian3.angleBetween(from, to);
      if (angle >= Math.PI - 0.02) return null;
      if (Cartesian3.distance(from, to) < 0.5) continue;
      const arc = new EllipsoidGeodesic(
        Cartographic.fromDegrees(...a),
        Cartographic.fromDegrees(...b),
      );
      const count = Math.min(128, Math.max(1, Math.ceil(arc.surfaceDistance / 50_000)));
      const needed = positions.length === 0 ? count + 1 : count;
      if (needed > remaining) return null;
      remaining -= needed;
      const lift = 50 + (arc.surfaceDistance / count) ** 2 / (8 * 6_371_007.1809);
      for (let step = 0; step <= count; step++) {
        const point = arc.interpolateUsingFraction(step / count);
        point.height = step === 0 ? Math.max(lift, lastLift) : lift;
        if (step === 0 && positions.length > 0)
          positions[positions.length - 1] = Cartographic.toCartesian(point);
        else positions.push(Cartographic.toCartesian(point));
      }
      lastLift = lift;
    }
    if (positions.length < 2) return null;
    budget.remaining = remaining;
    return positions;
  } catch {
    return null;
  }
}
