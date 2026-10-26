/**
 * Spatial Search-Around & Co-Location Graph (LB-140 Item 10 + LB-123 awareness cohorts,
 * ported from legacy/src/layers/awareness/model.js).
 */
import type { FeedId, FeedSnapshot, GeoPoint } from '../../contracts';
import { fireCellId, isValidFireCell } from '../../feeds/fireCells';
import type { SnapshotSource } from '../search/entitySearch';

export interface ProximityNeighbor {
  readonly feed: FeedId;
  readonly id: string;
  readonly label: string;
  readonly detail: string;
  readonly position: GeoPoint;
  readonly distanceKm: number;
  readonly bearingDeg: number;
  readonly cardinal: string;
}

export interface ProximityCohort {
  readonly feed: FeedId;
  readonly count: number;
}

export interface SearchAroundGraph {
  readonly radiusKm: number;
  readonly totalWithinRadius: number;
  readonly cohorts: readonly ProximityCohort[];
  readonly neighbors: readonly ProximityNeighbor[];
}

const EARTH_RADIUS_KM = 6_371.0088;
const DEG_TO_RAD = Math.PI / 180;
const RAD_TO_DEG = 180 / Math.PI;
const CARDINALS = ['N', 'NE', 'E', 'SE', 'S', 'SW', 'W', 'NW'] as const;

export function haversineKm(a: GeoPoint, b: GeoPoint): number {
  const dLat = (b.lat - a.lat) * DEG_TO_RAD;
  const dLon = (b.lon - a.lon) * DEG_TO_RAD;
  const lat1 = a.lat * DEG_TO_RAD;
  const lat2 = b.lat * DEG_TO_RAD;
  const sinLat = Math.sin(dLat / 2);
  const sinLon = Math.sin(dLon / 2);
  const h = sinLat * sinLat + Math.cos(lat1) * Math.cos(lat2) * sinLon * sinLon;
  return 2 * EARTH_RADIUS_KM * Math.asin(Math.min(1, Math.sqrt(h)));
}

export function initialBearingDeg(from: GeoPoint, to: GeoPoint): number {
  const lat1 = from.lat * DEG_TO_RAD;
  const lat2 = to.lat * DEG_TO_RAD;
  const dLon = (to.lon - from.lon) * DEG_TO_RAD;
  const y = Math.sin(dLon) * Math.cos(lat2);
  const x = Math.cos(lat1) * Math.sin(lat2) - Math.sin(lat1) * Math.cos(lat2) * Math.cos(dLon);
  const brng = Math.atan2(y, x) * RAD_TO_DEG;
  return (Math.round(brng) + 360) % 360;
}

export function cardinalDirection(bearingDeg: number): string {
  const idx = Math.round((((bearingDeg % 360) + 360) % 360) / 45) % 8;
  return CARDINALS[idx] ?? 'N';
}

interface Candidate {
  readonly feed: FeedId;
  readonly id: string;
  readonly label: string;
  readonly detail: string;
  readonly position: GeoPoint;
}

function candidatesFromSnapshot(snapshot: FeedSnapshot): Candidate[] {
  const { items } = snapshot;
  switch (items.kind) {
    case 'aircraft':
      return items.items.map((a) => ({
        feed: 'aircraft' as const,
        id: a.hex,
        label: a.callsign?.trim() || a.hex,
        detail: [a.typeCode, a.registration].filter(Boolean).join(' · ') || 'Aircraft',
        position: a.position,
      }));
    case 'vessels':
      return items.items.map((v) => ({
        feed: 'vessels' as const,
        id: v.mmsi,
        label: v.name?.trim() || v.mmsi,
        detail: v.vesselType?.trim() || `MMSI ${v.mmsi}`,
        position: v.position,
      }));
    case 'earthquakes':
      return items.items.map((q) => ({
        feed: 'earthquakes' as const,
        id: q.id,
        label: `M ${q.magnitude.toFixed(1)}`,
        detail: q.place ?? q.id,
        position: q.position,
      }));
    case 'cyclones':
      return items.items.map((c) => ({
        feed: 'cyclones' as const,
        id: c.id,
        label: c.name,
        detail: `${c.classification} · ${c.basin}`,
        position: c.position,
      }));
    case 'fires':
      return items.items
        .slice(0, 250)
        .filter((f) => isValidFireCell(f))
        .map((f) => ({
          feed: 'fires' as const,
          id: fireCellId(f),
          label: `Fire ${f.frp.toFixed(0)} MW`,
          detail: f.satellite || 'Satellite',
          position: { lat: f.lat, lon: f.lon },
        }));
    case 'launches':
      return items.items.flatMap((l) =>
        l.pad
          ? [
              {
                feed: 'launches' as const,
                id: l.id,
                label: l.name,
                detail: l.provider ?? 'Launch',
                position: { lat: l.pad.latitude, lon: l.pad.longitude },
              },
            ]
          : [],
      );
    case 'bikeshare':
      return items.items.slice(0, 200).map((b) => ({
        feed: 'bikeshare' as const,
        id: b.id,
        label: b.name,
        detail: `${b.bikesAvailable} bikes`,
        position: b.position,
      }));
    case 'transit':
      return items.items.slice(0, 200).map((t) => ({
        feed: 'transit' as const,
        id: t.id,
        label: t.routeId ? `Route ${t.routeId}` : t.vehicleId,
        detail: `Vehicle ${t.vehicleId}`,
        position: t.position,
      }));
    default:
      return [];
  }
}

const SPATIAL_FEEDS: readonly FeedId[] = [
  'aircraft',
  'vessels',
  'earthquakes',
  'cyclones',
  'fires',
  'launches',
  'bikeshare',
  'transit',
];

export function computeSearchAroundGraph(
  source: SnapshotSource,
  subject: { readonly feed: string; readonly id: string; readonly position: GeoPoint },
  radiusKm: number,
  maxNeighbors = 8,
): SearchAroundGraph {
  const neighbors: ProximityNeighbor[] = [];
  const cohortCounts = new Map<FeedId, number>();

  for (const feed of SPATIAL_FEEDS) {
    const snapshot = source.latest(feed);
    if (!snapshot) continue;
    const candidates = candidatesFromSnapshot(snapshot);
    let countInFeed = 0;
    for (const c of candidates) {
      if (c.feed === subject.feed && c.id === subject.id) continue;
      const distKm = haversineKm(subject.position, c.position);
      if (distKm <= radiusKm) {
        countInFeed += 1;
        const bearing = initialBearingDeg(subject.position, c.position);
        neighbors.push({
          ...c,
          distanceKm: distKm,
          bearingDeg: bearing,
          cardinal: cardinalDirection(bearing),
        });
      }
    }
    if (countInFeed > 0) {
      cohortCounts.set(feed, countInFeed);
    }
  }

  neighbors.sort((a, b) => a.distanceKm - b.distanceKm);
  const cohorts: ProximityCohort[] = [...cohortCounts.entries()].map(([feed, count]) => ({
    feed,
    count,
  }));

  return {
    radiusKm,
    totalWithinRadius: neighbors.length,
    cohorts,
    neighbors: neighbors.slice(0, maxNeighbors),
  };
}
