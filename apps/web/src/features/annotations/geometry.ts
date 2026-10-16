import type { GeoPoint } from '../../contracts';

/** Authalic sphere: surface estimates, without terrain or ellipsoid correction. */
export const EARTH_RADIUS_M = 6_371_007.1809;
/** Cesium Vincenty solver rejects angles within 0.0125 rad of π; keep a wider margin. */
export const MAX_EDGE_ANGLE_RAD = Math.PI - 0.02;
type Vector = readonly [number, number, number];
const radians = (degrees: number) => (degrees * Math.PI) / 180;
const dot = (a: Vector, b: Vector) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a: Vector, b: Vector): Vector => [
  a[1] * b[2] - a[2] * b[1],
  a[2] * b[0] - a[0] * b[2],
  a[0] * b[1] - a[1] * b[0],
];
const norm = (a: Vector) => Math.hypot(...a);
const unit = (a: Vector): Vector => {
  const length = norm(a);
  return [a[0] / length, a[1] / length, a[2] / length];
};
const vector = (p: GeoPoint): Vector => {
  const lat = radians(p.lat);
  const lon = radians(p.lon);
  return [Math.cos(lat) * Math.cos(lon), Math.cos(lat) * Math.sin(lon), Math.sin(lat)];
};
const angle = (a: Vector, b: Vector) => Math.atan2(norm(cross(a, b)), dot(a, b));

export function greatCircleM(a: GeoPoint, b: GeoPoint): number {
  return EARTH_RADIUS_M * angle(vector(a), vector(b));
}

export function pathLengthM(points: readonly GeoPoint[]): number {
  return points.slice(1).reduce((sum, p, i) => sum + greatCircleM(points[i] as GeoPoint, p), 0);
}

function centreVector(points: readonly GeoPoint[]): Vector {
  const total = points
    .map(vector)
    .reduce<Vector>((sum, v) => [sum[0] + v[0], sum[1] + v[1], sum[2] + v[2]], [0, 0, 0]);
  return unit(total);
}

export function annotationCentre(points: readonly GeoPoint[]): GeoPoint | null {
  if (!points.length) return null;
  const v = centreVector(points);
  if (!v.every(Number.isFinite)) return points[0] ?? null;
  return {
    lat: (Math.atan2(v[2], Math.hypot(v[0], v[1])) * 180) / Math.PI,
    lon: (Math.atan2(v[1], v[0]) * 180) / Math.PI,
  };
}

/** Signed triangle solid angles, independent of longitude wrapping and safe at poles. */
export function ringAreaM2(points: readonly GeoPoint[]): number {
  if (points.length < 3) return 0;
  const c = centreVector(points);
  const vertices = points.map(vector);
  let steradians = 0;
  for (let i = 0; i < vertices.length; i++) {
    const a = vertices[i] as Vector;
    const b = vertices[(i + 1) % vertices.length] as Vector;
    steradians += 2 * Math.atan2(dot(c, cross(a, b)), 1 + dot(c, a) + dot(a, b) + dot(b, c));
  }
  return Math.abs(steradians) * EARTH_RADIUS_M ** 2;
}

const onArc = (p: Vector, a: Vector, b: Vector) =>
  Math.abs(angle(a, p) + angle(p, b) - angle(a, b)) < 1e-8;

/** Simple short-arc rings inside one hemisphere. Ambiguous or crossing rings are refused. */
export function geometryError(
  kind: 'pin' | 'line' | 'area',
  points: readonly GeoPoint[],
): string | null {
  const minimum = { pin: 1, line: 2, area: 3 }[kind];
  if (points.length < minimum) return `Add at least ${minimum} point${minimum === 1 ? '' : 's'}.`;
  if (kind === 'pin') return points.length === 1 ? null : 'A pin needs one point.';
  const vertices = points.map(vector);
  const edges = kind === 'area' ? vertices.length : vertices.length - 1;
  for (let i = 0; i < edges; i++) {
    const distance =
      angle(vertices[i] as Vector, vertices[(i + 1) % vertices.length] as Vector) * EARTH_RADIUS_M;
    if (distance < 0.5) return 'Adjacent points must be at least 0.5 m apart.';
    if (distance >= MAX_EDGE_ANGLE_RAD * EARTH_RADIUS_M)
      return 'Near-antipodal edges are ambiguous. Add an intermediate point.';
  }
  if (kind === 'line')
    return pathLengthM(points) >= 1 ? null : 'The line needs at least 1 m of length.';
  const centre = centreVector(points);
  if (!centre.every(Number.isFinite) || vertices.some((v) => dot(v, centre) < 0.05))
    return 'Areas must fit within one hemisphere. Split this area into smaller marks.';
  for (let i = 0; i < edges; i++) {
    const a = vertices[i] as Vector;
    const b = vertices[(i + 1) % edges] as Vector;
    for (let j = i + 1; j < edges; j++) {
      if (j === i + 1 || (i === 0 && j === edges - 1)) continue;
      const c = vertices[j] as Vector;
      const d = vertices[(j + 1) % edges] as Vector;
      const intersection = cross(cross(a, b), cross(c, d));
      if (norm(intersection) < 1e-12) {
        if (onArc(a, c, d) || onArc(b, c, d) || onArc(c, a, b) || onArc(d, a, b))
          return 'Area edges must not cross or overlap.';
      } else {
        const p = unit(intersection);
        const opposite: Vector = [-p[0], -p[1], -p[2]];
        if ([p, opposite].some((v) => onArc(v, a, b) && onArc(v, c, d)))
          return 'Area edges must not cross or overlap.';
      }
    }
  }
  return ringAreaM2(points) >= 1 ? null : 'The area needs at least 1 m² of surface.';
}

export function measurement(kind: 'pin' | 'line' | 'area', points: readonly GeoPoint[]): string {
  if (kind === 'pin') return 'User pin';
  if (kind === 'line') {
    const length = pathLengthM(points);
    return `Approximate ${length >= 1_000 ? `${(length / 1_000).toFixed(2)} km` : `${length.toFixed(1)} m`}`;
  }
  const area = ringAreaM2(points);
  return `Approximate ${area >= 1e6 ? `${(area / 1e6).toFixed(2)} km²` : `${area.toFixed(1)} m²`}`;
}
