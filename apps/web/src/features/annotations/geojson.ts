/**
 * RFC 7946 GeoJSON import/export for workspace annotations — ported from
 * legacy/src/data/geoFileImport.js and legacy/src/ui/geoFileImport.js.
 */
import type { GeoPoint } from '../../contracts';
import { geometryError, greatCircleM } from './geometry';
import {
  MAX_ANNOTATIONS,
  MAX_LABEL_LENGTH,
  MAX_NOTE_LENGTH,
  MAX_VERTICES,
  type MapAnnotation,
  validatePoints,
} from './model';

const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);

const sanitizeText = (value: unknown, maxLen: number): string => {
  if (typeof value !== 'string') return '';
  return value
    .replace(/\p{Cc}/gu, ' ')
    .trim()
    .slice(0, maxLen);
};

const roundCoord = (point: GeoPoint): GeoPoint => ({
  lat: Number(point.lat.toFixed(6)),
  lon: Number(point.lon.toFixed(6)),
});

function parsePosition(coord: unknown): GeoPoint {
  if (!Array.isArray(coord) || coord.length < 2) {
    throw new Error('GeoJSON coordinates must be [longitude, latitude] pairs.');
  }
  const lon = Number(coord[0]);
  const lat = Number(coord[1]);
  const [validated] = validatePoints([{ lat, lon }]);
  if (!validated) throw new Error('Invalid coordinate pair.');
  return roundCoord(validated);
}

function dedupeConsecutivePoints(points: readonly GeoPoint[]): GeoPoint[] {
  const result: GeoPoint[] = [];
  for (const pt of points) {
    const prev = result.at(-1);
    if (!prev || greatCircleM(prev, pt) >= 0.5) {
      result.push(pt);
    }
  }
  return result;
}

function featureFromGeometry(
  geometry: Record<string, unknown>,
  properties: Record<string, unknown>,
  nowIso: string,
): MapAnnotation | null {
  const type = geometry.type;
  const coords = geometry.coordinates;
  const rawLabel = properties.label ?? properties.name ?? properties.title ?? '';
  const rawNote = properties.note ?? properties.description ?? '';
  const label = sanitizeText(rawLabel, MAX_LABEL_LENGTH);
  const note = sanitizeText(rawNote, MAX_NOTE_LENGTH);
  const requestedKind = properties.kind;

  if (type === 'Point') {
    const pt = parsePosition(coords);
    return {
      id: crypto.randomUUID(),
      kind: 'pin',
      points: [pt],
      label,
      note,
      createdAt: nowIso,
      approximate: true,
    };
  }

  if (type === 'LineString') {
    if (!Array.isArray(coords)) throw new Error('GeoJSON LineString requires a coordinate array.');
    const pts = dedupeConsecutivePoints(coords.slice(0, MAX_VERTICES).map(parsePosition));
    const problem = geometryError('line', pts);
    if (problem) throw new Error(problem);
    return {
      id: crypto.randomUUID(),
      kind: 'line',
      points: pts,
      label,
      note,
      createdAt: nowIso,
      approximate: true,
    };
  }

  if (type === 'Polygon') {
    if (!Array.isArray(coords) || !Array.isArray(coords[0])) {
      throw new Error('GeoJSON Polygon requires an outer linear ring.');
    }
    const ring = coords[0] as unknown[];
    const parsedRing = ring.map(parsePosition);
    // Strip closing vertex if it duplicates the first vertex
    if (
      parsedRing.length > 1 &&
      parsedRing[0] &&
      parsedRing.at(-1) &&
      greatCircleM(parsedRing[0], parsedRing.at(-1) as GeoPoint) < 0.5
    ) {
      parsedRing.pop();
    }
    const pts = dedupeConsecutivePoints(parsedRing.slice(0, MAX_VERTICES));
    const kind: 'area' | 'geofence' = requestedKind === 'geofence' ? 'geofence' : 'area';
    const problem = geometryError(kind, pts);
    if (problem) throw new Error(problem);
    return {
      id: crypto.randomUUID(),
      kind,
      points: pts,
      label,
      note,
      createdAt: nowIso,
      approximate: true,
    };
  }

  return null;
}

/**
 * Parse a GeoJSON string (`FeatureCollection`, `Feature`, or bare geometry) into validated
 * `MapAnnotation` items ready to merge into the workspace.
 */
export function parseGeoJsonToAnnotations(
  raw: string,
  existingCount = 0,
): readonly MapAnnotation[] {
  const remaining = Math.max(0, MAX_ANNOTATIONS - existingCount);
  if (remaining === 0) {
    throw new Error(`Keep at most ${MAX_ANNOTATIONS} annotations.`);
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    throw new Error('Invalid JSON file. Choose a valid GeoJSON document.');
  }
  if (!isRecord(parsed)) {
    throw new Error('GeoJSON root must be an object.');
  }

  const nowIso = new Date().toISOString();
  const out: MapAnnotation[] = [];

  if (parsed.type === 'FeatureCollection') {
    if (!Array.isArray(parsed.features)) {
      throw new Error('GeoJSON FeatureCollection requires a features array.');
    }
    for (const f of parsed.features) {
      if (out.length >= remaining) break;
      if (!isRecord(f) || !isRecord(f.geometry)) continue;
      const props = isRecord(f.properties) ? f.properties : {};
      const item = featureFromGeometry(f.geometry, props, nowIso);
      if (item) out.push(item);
    }
  } else if (parsed.type === 'Feature') {
    if (!isRecord(parsed.geometry)) {
      throw new Error('GeoJSON Feature requires a geometry object.');
    }
    const props = isRecord(parsed.properties) ? parsed.properties : {};
    const item = featureFromGeometry(parsed.geometry, props, nowIso);
    if (item) out.push(item);
  } else {
    const item = featureFromGeometry(parsed, {}, nowIso);
    if (item) out.push(item);
  }

  if (out.length === 0) {
    throw new Error('No supported Point, LineString, or Polygon features found in GeoJSON.');
  }
  return out;
}

/** Serialize workspace annotations to an RFC 7946 GeoJSON FeatureCollection string. */
export function exportAnnotationsToGeoJson(annotations: readonly MapAnnotation[]): string {
  const features = annotations.map((a) => {
    const properties = {
      id: a.id,
      kind: a.kind,
      label: a.label,
      note: a.note,
      createdAt: a.createdAt,
      approximate: a.approximate,
    };
    if (a.kind === 'pin') {
      const pt = a.points[0] ?? { lat: 0, lon: 0 };
      return {
        type: 'Feature',
        properties,
        geometry: {
          type: 'Point',
          coordinates: [pt.lon, pt.lat],
        },
      };
    }
    if (a.kind === 'line') {
      return {
        type: 'Feature',
        properties,
        geometry: {
          type: 'LineString',
          coordinates: a.points.map((p) => [p.lon, p.lat]),
        },
      };
    }
    const ring = a.points.map((p) => [p.lon, p.lat]);
    const first = a.points[0];
    if (first) {
      ring.push([first.lon, first.lat]);
    }
    return {
      type: 'Feature',
      properties,
      geometry: {
        type: 'Polygon',
        coordinates: [ring],
      },
    };
  });

  return JSON.stringify(
    {
      type: 'FeatureCollection',
      features,
    },
    null,
    2,
  );
}
