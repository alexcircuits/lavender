import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  catalogOf,
  FakePlatform,
  fakeGlobe,
  imagery,
  layer,
  memoryPreferences,
} from '../test/fakes';
import { createRuntime } from './createRuntime';
import { readWorkspace, WORKSPACE_PREF_KEY } from './workspacePrefs';

const catalog = catalogOf([
  layer('earthquakes', { defaultOn: true }),
  layer('aircraft'),
  layer('satellites'),
]);

describe('createRuntime', () => {
  let platform: FakePlatform;

  beforeEach(() => {
    vi.useFakeTimers();
    platform = new FakePlatform();
    platform.catalogResult = catalog;
  });

  afterEach(() => vi.useRealTimers());

  it('loads the catalog, starts default layers and stops cleanly', async () => {
    const runtime = createRuntime(platform);
    const stop = runtime.start();
    expect(runtime.start()).toBe(stop);
    await vi.advanceTimersByTimeAsync(0);
    expect(runtime.stores.system.getState().catalog).toBe(catalog);
    expect(platform.live('earthquakes')).toBeDefined();
    expect(platform.live('aircraft')).toBeUndefined();
    stop();
    expect(platform.subs.every((s) => s.closed)).toBe(true);
  });

  it('restores persisted layers instead of the defaults', async () => {
    platform.preferences = memoryPreferences({
      [WORKSPACE_PREF_KEY]: {
        version: 1,
        layers: { aircraft: { enabled: true, filters: { militaryOnly: true } } },
        imageryId: 'night',
      },
    });
    const runtime = createRuntime(platform);
    runtime.start();
    await vi.advanceTimersByTimeAsync(0);
    expect(platform.live('earthquakes')).toBeUndefined();
    expect(platform.live('aircraft')?.query.filters).toEqual({ militaryOnly: true });
    expect(runtime.activeImagery()?.id).toBe('night');
  });

  it('retries a failed catalog with backoff and records the error', async () => {
    platform.catalogResult = new Error('backend down');
    const runtime = createRuntime(platform, { catalogRetryMs: { min: 100, max: 400 } });
    runtime.start();
    await vi.advanceTimersByTimeAsync(0);
    expect(runtime.stores.system.getState().catalogError?.message).toMatch(/could not reach/);
    platform.catalogResult = catalog;
    await vi.advanceTimersByTimeAsync(100);
    expect(runtime.stores.system.getState()).toMatchObject({ catalog, catalogError: null });
    expect(platform.catalog).toHaveBeenCalledTimes(2);
  });

  it('persists workspace changes after a quiet period', async () => {
    const runtime = createRuntime(platform, { persistDelayMs: 200 });
    runtime.start();
    await vi.advanceTimersByTimeAsync(0);
    runtime.stores.workspace.getState().setLayerEnabled('aircraft', true);
    runtime.stores.workspace.getState().setFilters('aircraft', { militaryOnly: true });
    expect(platform.preferences.data.has(WORKSPACE_PREF_KEY)).toBe(false);
    await vi.advanceTimersByTimeAsync(200);
    const saved = platform.preferences.data.get(WORKSPACE_PREF_KEY);
    expect(readWorkspace(saved, catalog).layers.aircraft).toEqual({
      enabled: true,
      filters: { militaryOnly: true },
    });
  });

  it('gives an attached globe the imagery and follows imagery changes', async () => {
    const runtime = createRuntime(platform);
    runtime.start();
    const { globe } = fakeGlobe();
    runtime.attachGlobe(globe);
    expect(runtime.stores.ui.getState().globe).toBe(globe);
    expect(globe.setImagery).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(0);
    expect(globe.setImagery).toHaveBeenLastCalledWith(expect.objectContaining({ id: 'offline' }));
    runtime.stores.workspace.getState().setImagery('night');
    expect(globe.setImagery).toHaveBeenLastCalledWith(expect.objectContaining({ id: 'night' }));
    runtime.detachGlobe();
    expect(runtime.stores.ui.getState().globe).toBeNull();
  });

  it('polls backend status', async () => {
    platform.statusResult = {
      version: '0.1.0',
      licenceMode: 'evaluation',
      startedAt: '2026-10-11T00:00:00Z',
      sources: [],
      feeds: [],
    };
    const runtime = createRuntime(platform, { statusIntervalMs: 1_000 });
    runtime.start();
    await vi.advanceTimersByTimeAsync(2_500);
    expect(platform.status).toHaveBeenCalledTimes(3);
    expect(runtime.stores.system.getState().status?.version).toBe('0.1.0');
  });
});

describe('readWorkspace', () => {
  it('drops unknown feeds, bad filters and unknown imagery', () => {
    const read = readWorkspace(
      {
        version: 1,
        layers: {
          earthquakes: {
            enabled: true,
            filters: { minMagnitude: 99, groups: ['ok', 'Bad Group!'] },
          },
          ships: { enabled: true, filters: {} },
          aircraft: { enabled: 'yes' },
        },
        imageryId: 'nope',
      },
      catalogOf([layer('earthquakes'), layer('aircraft')], {
        imagery: [imagery('a', { isDefault: true })],
      }),
    );
    expect(read).toEqual({
      version: 1,
      layers: { earthquakes: { enabled: true, filters: { groups: ['ok'] } } },
      imageryId: null,
    });
  });

  it('falls back to defaults for other versions or garbage', () => {
    const c = catalogOf([layer('earthquakes', { defaultOn: true })]);
    expect(readWorkspace('x', c).layers).toEqual({ earthquakes: { enabled: true, filters: {} } });
    expect(readWorkspace({ version: 2, layers: {} }, c).layers).toEqual({
      earthquakes: { enabled: true, filters: {} },
    });
  });
});
