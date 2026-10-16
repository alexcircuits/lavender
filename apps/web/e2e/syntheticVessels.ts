/**
 * A deterministic synthetic AIS picture for performance runs: SYNTHETIC, invented ships only.
 * Positions, speeds and courses come from a seeded PRNG (mulberry32), names read
 * "SYNTH 00001…" and MMSIs use the unallocated 9xx MID range, so nothing can be mistaken
 * for a real vessel. The snapshot's notice says so.
 */
import type { FeedSnapshot, Vessel } from '../src/contracts';

export const SYNTHETIC_NOTICE = 'Synthetic performance fixture: invented vessels, not live.';

/** mulberry32: a small, well-distributed 32-bit PRNG (public domain). */
function prng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4_294_967_296;
  };
}

export interface Box {
  readonly west: number;
  readonly south: number;
  readonly east: number;
  readonly north: number;
}

/** The southern North Sea: a dense regional picture that fits one 900 km-high view. */
export const NORTH_SEA: readonly Box[] = [{ west: 0, south: 52.5, east: 8, north: 56.5 }];

/** Busy sea areas worldwide, for a whole-globe picture. Boxes are rough and may touch land. */
export const WORLD_SEAS: readonly Box[] = [
  { west: -10, south: 36, east: 30, north: 44 }, // Mediterranean
  { west: -2, south: 51, east: 9, north: 58.5 }, // North Sea
  { west: 10, south: 54, east: 28, north: 60 }, // Baltic
  { west: -75, south: 25, east: -10, north: 45 }, // North Atlantic
  { west: 50, south: 0, east: 100, north: 22 }, // Arabian Sea, Bay of Bengal
  { west: 100, south: 0, east: 125, north: 35 }, // Malacca, South and East China Seas
  { west: 125, south: 25, east: 150, north: 45 }, // Japan
  { west: -130, south: 20, east: -75, north: 48 }, // US coasts, Gulf of Mexico
  { west: -60, south: -40, east: -30, north: 0 }, // South America
  { west: 110, south: -40, east: 155, north: -10 }, // Australia
];

const TYPES = ['Cargo', 'Tanker', 'Passenger', 'Fishing', 'Tug', 'Pleasure craft'] as const;
const PORTS = ['SYNTH A', 'SYNTH B', 'SYNTH C', 'SYNTH D'] as const;

/**
 * `count` invented vessels spread over `boxes`; about 70 % under way, 30 % at rest. Every
 * ship carries the static fields the stream sends (name, IMO, type, destination), so the
 * payload is the worst case for a busy view; voyage details are not streamed (they come
 * with the selected ship's track).
 */
export function syntheticVessels(count: number, boxes: readonly Box[], seed = 980): Vessel[] {
  const random = prng(seed);
  const at = Date.parse('2026-10-16T00:00:00Z') / 1000;
  const pick = <T>(values: readonly T[]): T => values[Math.floor(random() * values.length)] as T;
  return Array.from({ length: count }, (_, i): Vessel => {
    const box = boxes[Math.floor(random() * boxes.length)] ?? boxes[0];
    if (!box) throw new Error('no box');
    const lat = box.south + random() * (box.north - box.south);
    const lon = box.west + random() * (box.east - box.west);
    const underway = random() < 0.7;
    const course = Math.round(random() * 3599) / 10;
    const n = String(i + 1).padStart(5, '0');
    return {
      mmsi: `9${String(i + 1).padStart(8, '0')}`,
      name: `SYNTH ${n}`,
      imo: '9999993',
      vesselType: pick(TYPES),
      destination: pick(PORTS),
      position: { lat: Math.round(lat * 1e5) / 1e5, lon: Math.round(lon * 1e5) / 1e5 },
      speedKnots: underway ? Math.round((1 + random() * 24) * 10) / 10 : 0,
      courseDeg: course,
      headingDeg: Math.round(course),
      lastPositionUtc: new Date(at * 1000).toISOString(),
      lastPositionEpoch: at,
    };
  });
}

/** The same ships `seconds` later, each moved along its course (an update with every id). */
export function advanced(vessels: readonly Vessel[], seconds: number): Vessel[] {
  return vessels.map((v) => {
    const knots = v.speedKnots ?? 0;
    if (knots === 0) return v;
    const metres = knots * 0.514_444 * seconds;
    const course = ((v.courseDeg ?? 0) * Math.PI) / 180;
    const dLat = (metres * Math.cos(course)) / 111_320;
    const dLon =
      (metres * Math.sin(course)) /
      (111_320 * Math.max(0.01, Math.cos((v.position.lat * Math.PI) / 180)));
    const lon = ((v.position.lon + dLon + 540) % 360) - 180;
    return {
      ...v,
      position: { lat: Math.max(-89, Math.min(89, v.position.lat + dLat)), lon },
      lastPositionEpoch: (v.lastPositionEpoch ?? 0) + seconds,
    };
  });
}

export function syntheticSnapshot(vessels: Vessel[]): FeedSnapshot {
  const now = new Date().toISOString();
  return {
    feed: 'vessels',
    sources: ['aisstream'],
    fetchedAt: now,
    generatedAt: now,
    freshness: { state: 'live' },
    coverage: { kind: 'complete' },
    sourceCount: vessels.length,
    pendingRegions: 0,
    items: { kind: 'vessels', items: vessels },
    notices: [SYNTHETIC_NOTICE],
  };
}
