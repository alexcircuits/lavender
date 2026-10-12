/**
 * Test harness for the interface: a real runtime over the scriptable FakePlatform, rendered
 * inside the same providers as the app. Test-only; nothing in the app imports it.
 */
import { act, type RenderResult, render } from '@testing-library/react';
import type { ReactNode } from 'react';
import type {
  Aircraft,
  Catalog,
  FeedSnapshot,
  SatelliteElements,
  SourceInfo,
  SystemStatus,
} from '../contracts';
import { DesignProvider } from '../design';
import { createRuntime, type Runtime, RuntimeProvider } from '../runtime';
import { type FakeGlobe, FakePlatform, fakeGlobe } from '../test/fakes';

export interface Harness {
  readonly platform: FakePlatform;
  readonly runtime: Runtime;
  readonly globe: FakeGlobe | null;
  readonly stop: () => void;
}

export interface HarnessOptions {
  readonly catalog?: Catalog | Error;
  readonly status?: SystemStatus;
  /** Attach a recording globe (map controls and fly-to need one). */
  readonly globe?: boolean;
  /** Start the runtime and wait for the catalog (default true). */
  readonly start?: boolean;
}

/** A started runtime with its catalog loaded and, optionally, a recording globe attached. */
export async function startHarness(options: HarnessOptions = {}): Promise<Harness> {
  const platform = new FakePlatform();
  if (options.catalog) platform.catalogResult = options.catalog;
  if (options.status) platform.statusResult = options.status;
  const runtime = createRuntime(platform, { persistDelayMs: 0 });
  let stop: () => void = () => undefined;
  if (options.start !== false) {
    await act(async () => {
      stop = runtime.start();
    });
  }
  const globe = options.globe ? fakeGlobe() : null;
  if (globe) act(() => runtime.attachGlobe(globe.globe));
  return { platform, runtime, globe, stop: () => stop() };
}

export function renderWithRuntime(runtime: Runtime, ui: ReactNode): RenderResult {
  return render(
    <RuntimeProvider runtime={runtime}>
      <DesignProvider>{ui}</DesignProvider>
    </RuntimeProvider>,
  );
}

/** Delivers a snapshot to the open subscription for its feed. */
export function deliver(harness: Harness, snapshot: FeedSnapshot): void {
  const sub = harness.platform.live(snapshot.feed);
  if (!sub) throw new Error(`No open subscription for ${snapshot.feed}`);
  act(() => sub.onEvent({ kind: 'snapshot', snapshot }));
}

export function source(id: string, overrides: Partial<SourceInfo> = {}): SourceInfo {
  return {
    id,
    name: id,
    homepage: `https://${id}.example.org/`,
    attribution: `Data from ${id}`,
    licence: { name: 'CC BY 4.0', url: 'https://creativecommons.org/licenses/by/4.0/' },
    commercial: 'allowed',
    requiresKey: false,
    privacy: 'normal',
    ...overrides,
  };
}

export function aircraft(hex: string, overrides: Partial<Aircraft> = {}): Aircraft {
  return {
    hex,
    position: { lat: 60.1699, lon: 24.9384 },
    onGround: false,
    isMilitary: false,
    seenPosS: 2,
    source: 'adsb-lol',
    ...overrides,
  };
}

export function aircraftSnapshot(
  items: Aircraft[],
  overrides: Partial<FeedSnapshot> = {},
): FeedSnapshot {
  return {
    feed: 'aircraft',
    sources: ['adsb-lol'],
    fetchedAt: '2026-10-11T12:00:00Z',
    generatedAt: '2026-10-11T12:00:01Z',
    freshness: { state: 'live' },
    coverage: { kind: 'complete' },
    sourceCount: items.length,
    pendingRegions: 0,
    items: { kind: 'aircraft', items },
    notices: [],
    ...overrides,
  };
}

export function satellite(
  noradId: number,
  overrides: Partial<SatelliteElements> = {},
): SatelliteElements {
  return {
    noradId,
    name: 'ISS (ZARYA)',
    intlDesignator: '1998-067A',
    group: 'stations',
    epoch: '2026-10-11T06:00:00Z',
    meanMotion: 15.5,
    eccentricity: 0.0004,
    inclinationDeg: 51.64,
    raanDeg: 10,
    argOfPericenterDeg: 20,
    meanAnomalyDeg: 30,
    bstar: 0.0001,
    meanMotionDot: 0,
    meanMotionDdot: 0,
    ...overrides,
  };
}

export function satelliteSnapshot(items: SatelliteElements[]): FeedSnapshot {
  return {
    feed: 'satellites',
    sources: ['celestrak'],
    fetchedAt: '2026-10-11T12:00:00Z',
    generatedAt: '2026-10-11T12:00:01Z',
    freshness: { state: 'live' },
    coverage: { kind: 'complete' },
    sourceCount: items.length,
    pendingRegions: 0,
    items: { kind: 'satellites', items },
    notices: [],
  };
}
