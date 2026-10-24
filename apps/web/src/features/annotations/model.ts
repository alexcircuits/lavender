import type { GeoPoint } from '../../contracts';
import { geometryError } from './geometry';

export const MAX_ANNOTATIONS = 24;
export const MAX_VERTICES = 32;
export const MAX_LABEL_LENGTH = 120;
export const MAX_NOTE_LENGTH = 500;
export const MAX_ANNOTATION_BYTES = 65_536;
export interface MapAnnotation {
  readonly id: string;
  /** `geofence` marks a named monitored region drawn with the geofence tool. */
  readonly kind: 'pin' | 'line' | 'area' | 'geofence';
  readonly points: readonly GeoPoint[];
  readonly label: string;
  readonly note: string;
  readonly createdAt: string;
  readonly approximate: true;
}
export interface AnnotationDraft {
  readonly kind: MapAnnotation['kind'];
  readonly points: readonly GeoPoint[];
  /** Live pointer position while drawing, for the next-segment preview. */
  readonly cursor?: GeoPoint | null;
}
const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);
function fields(v: unknown, keys: readonly string[]): Record<string, unknown> {
  if (!isRecord(v) || Object.keys(v).some((key) => !keys.includes(key)))
    throw new Error('Invalid annotation fields.');
  return v;
}
function boundedText(v: unknown, maximum: number): string {
  if (typeof v !== 'string' || v.length > maximum || /\p{Cc}/u.test(v))
    throw new Error('Annotation text is invalid or too long.');
  return v;
}
export function validatePoints(value: unknown): readonly GeoPoint[] {
  if (!Array.isArray(value) || value.length > MAX_VERTICES)
    throw new Error(`Use at most ${MAX_VERTICES} points.`);
  return value.map((point) => {
    const p = fields(point, ['lat', 'lon']);
    if (
      typeof p.lat !== 'number' ||
      !Number.isFinite(p.lat) ||
      Math.abs(p.lat) > 90 ||
      typeof p.lon !== 'number' ||
      !Number.isFinite(p.lon) ||
      Math.abs(p.lon) > 180
    )
      throw new Error('Coordinates must be finite latitude −90…90 and longitude −180…180.');
    return { lat: p.lat, lon: p.lon };
  });
}
export function validateAnnotations(value: unknown): readonly MapAnnotation[] {
  if (!Array.isArray(value) || value.length > MAX_ANNOTATIONS)
    throw new Error(`Keep at most ${MAX_ANNOTATIONS} annotations.`);
  const ids = new Set<string>();
  const annotations = value.map((entry): MapAnnotation => {
    const a = fields(entry, ['id', 'kind', 'points', 'label', 'note', 'createdAt', 'approximate']);
    if (typeof a.id !== 'string' || !/^[a-zA-Z0-9-]{1,64}$/.test(a.id) || ids.has(a.id))
      throw new Error('Invalid annotation identity.');
    ids.add(a.id);
    if (a.kind !== 'pin' && a.kind !== 'line' && a.kind !== 'area' && a.kind !== 'geofence')
      throw new Error('Unknown annotation kind.');
    if (
      a.approximate !== true ||
      typeof a.createdAt !== 'string' ||
      !/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z$/.test(a.createdAt) ||
      !Number.isFinite(Date.parse(a.createdAt)) ||
      new Date(a.createdAt).toISOString() !== a.createdAt
    )
      throw new Error('Invalid annotation metadata.');
    const points = validatePoints(a.points);
    const error = geometryError(a.kind, points);
    if (error) throw new Error(error);
    return {
      id: a.id,
      kind: a.kind,
      points,
      label: boundedText(a.label, MAX_LABEL_LENGTH),
      note: boundedText(a.note, MAX_NOTE_LENGTH),
      createdAt: a.createdAt,
      approximate: true,
    };
  });
  if (new TextEncoder().encode(JSON.stringify(annotations)).length > MAX_ANNOTATION_BYTES)
    throw new Error('Annotation storage is full. Shorten notes or simplify geometry.');
  return annotations;
}
