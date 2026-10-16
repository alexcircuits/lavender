import type { FeedSnapshot, Vessel } from '../contracts';

/** Synthetic AIS contact; no live provider or licensed data is used. */
export function vessel(overrides: Partial<Vessel> = {}): Vessel {
  return { mmsi: '123456789', position: { lat: 60.1, lon: 24.9 }, ...overrides };
}

export function vesselSnapshot(items: Vessel[]): FeedSnapshot {
  return {
    feed: 'vessels',
    sources: ['test-ais'],
    generatedAt: '2026-10-15T12:00:00Z',
    freshness: { state: 'live' },
    coverage: { kind: 'complete' },
    sourceCount: items.length,
    pendingRegions: 0,
    items: { kind: 'vessels', items },
    notices: [],
  };
}
