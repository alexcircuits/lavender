import type { FeedSnapshot, Launch, LaunchPad } from '../contracts';

/** Synthetic launch pads and launches for tests; not real Launch Library 2 records. */
export const CAPE_PAD: LaunchPad = {
  name: 'Test Complex 40',
  locationName: 'Test Cape, FL, USA',
  latitude: 28.561941,
  longitude: -80.5772,
};

export const WEST_PAD: LaunchPad = {
  name: 'Test Complex 4E',
  locationName: 'Test Base, CA, USA',
  latitude: 34.632,
  longitude: -120.611,
};

export function launch(id: string, overrides: Partial<Launch> = {}): Launch {
  return {
    id,
    name: `Test Rocket | Mission ${id}`,
    status: 'Go for Launch',
    net: '2026-06-02T18:00:00Z',
    provider: 'Test Provider',
    rocket: 'Test Rocket Block 1',
    mission: `Mission ${id}`,
    pad: CAPE_PAD,
    payloadNames: [],
    matchedNoradIds: [],
    ...overrides,
  };
}

/** A launch without a pad (LL2 can omit it, or the server drops one it cannot place). */
export function padlessLaunch(id: string, overrides: Partial<Launch> = {}): Launch {
  const { pad: _pad, ...rest } = launch(id, overrides);
  return rest;
}

export function launchSnapshot(
  items: Launch[],
  overrides: Partial<FeedSnapshot> = {},
): FeedSnapshot {
  return {
    feed: 'launches',
    sources: ['launch-library-2'],
    fetchedAt: '2026-06-02T11:59:00Z',
    generatedAt: '2026-06-02T11:59:01Z',
    freshness: { state: 'live' },
    coverage: { kind: 'complete' },
    sourceCount: items.length,
    pendingRegions: 0,
    items: { kind: 'launches', items },
    notices: [],
    ...overrides,
  };
}
