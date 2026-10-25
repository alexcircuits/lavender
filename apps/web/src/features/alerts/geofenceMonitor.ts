import type { GeoPoint } from '../../contracts';
import type { EntitySource } from '../../globe';
import { pointInPolygonWasm } from '../../globe/renderers/wasmGeometry';
import type { MapAnnotation } from '../annotations/model';
import type { AlertItem } from './alertsStore';

export interface EntityRecord {
  readonly id: string;
  readonly feed?: EntitySource;
  readonly position?: GeoPoint;
  readonly lat?: number;
  readonly lon?: number;
  readonly label?: string;
  readonly name?: string;
  readonly callsign?: string;
}

export type GeofenceTrackingMap = Map<string, boolean>;

export interface GeofenceMonitorResult {
  readonly alerts: readonly AlertItem[];
  readonly nextTracking: GeofenceTrackingMap;
}

export function makeTrackingKey(geofenceId: string, entityId: string): string {
  return `${geofenceId}:${entityId}`;
}

function wasInside(
  tracking: ReadonlyMap<string, boolean> | Record<string, boolean> | undefined,
  geofenceId: string,
  entityId: string,
): boolean {
  if (!tracking) return false;
  if (tracking instanceof Map) {
    if (tracking.has(`${geofenceId}:${entityId}`)) {
      return tracking.get(`${geofenceId}:${entityId}`) === true;
    }
    if (tracking.has(`${entityId}:${geofenceId}`)) {
      return tracking.get(`${entityId}:${geofenceId}`) === true;
    }
    return false;
  }
  const record = tracking as Record<string, boolean>;
  return (
    record[`${geofenceId}:${entityId}`] === true || record[`${entityId}:${geofenceId}`] === true
  );
}

function resolveCoordinates(entity: EntityRecord): { lat: number; lon: number } | null {
  if (
    entity.position &&
    Number.isFinite(entity.position.lat) &&
    Number.isFinite(entity.position.lon)
  ) {
    return { lat: entity.position.lat, lon: entity.position.lon };
  }
  if (
    typeof entity.lat === 'number' &&
    Number.isFinite(entity.lat) &&
    typeof entity.lon === 'number' &&
    Number.isFinite(entity.lon)
  ) {
    return { lat: entity.lat, lon: entity.lon };
  }
  return null;
}

/**
 * Pure monitor function that checks feed entities against geofence annotations.
 * Uses Zig WASM point-in-polygon ray casting to evaluate containment.
 * Emits 'enter' when an entity transitions from outside to inside,
 * and 'exit' when transitioning from inside to outside.
 */
export function evaluateGeofences(
  entities: readonly EntityRecord[],
  geofences: readonly MapAnnotation[],
  previousTracking?: ReadonlyMap<string, boolean> | Record<string, boolean>,
  timestamp = Date.now(),
): GeofenceMonitorResult {
  const alerts: AlertItem[] = [];
  const nextTracking = new Map<string, boolean>(
    previousTracking instanceof Map
      ? previousTracking
      : previousTracking
        ? Object.entries(previousTracking)
        : [],
  );

  const activeGeofences = geofences.filter((g) => {
    if ('kind' in g && g.kind !== 'geofence' && g.kind !== 'area') {
      return false;
    }
    return Array.isArray(g.points) && g.points.length >= 3;
  });

  if (activeGeofences.length === 0 || entities.length === 0) {
    return { alerts, nextTracking };
  }

  for (const entity of entities) {
    const coords = resolveCoordinates(entity);
    if (!coords) continue;

    const entityId = entity.id;
    const feed = entity.feed ?? 'aircraft';
    const label = entity.label ?? entity.callsign ?? entity.name ?? entityId;

    for (const fence of activeGeofences) {
      const isInside = pointInPolygonWasm(coords.lon, coords.lat, fence.points);
      const previouslyInside = wasInside(previousTracking, fence.id, entityId);
      const key = makeTrackingKey(fence.id, entityId);

      nextTracking.set(key, isInside);

      if (!previouslyInside && isInside) {
        alerts.push({
          id: `alert-${fence.id}-${entityId}-enter-${timestamp}-${alerts.length}`,
          timestamp,
          type: 'enter',
          entity: { feed, id: entityId, label },
          geofenceId: fence.id,
          geofenceName: fence.label || 'Untitled geofence',
          position: { lat: coords.lat, lon: coords.lon },
          dismissed: false,
        });
      } else if (previouslyInside && !isInside) {
        alerts.push({
          id: `alert-${fence.id}-${entityId}-exit-${timestamp}-${alerts.length}`,
          timestamp,
          type: 'exit',
          entity: { feed, id: entityId, label },
          geofenceId: fence.id,
          geofenceName: fence.label || 'Untitled geofence',
          position: { lat: coords.lat, lon: coords.lon },
          dismissed: false,
        });
      }
    }
  }

  return { alerts, nextTracking };
}

export const monitorGeofences = evaluateGeofences;
