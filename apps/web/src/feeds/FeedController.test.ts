import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Catalog } from '../contracts';
import { createStores } from '../state/stores';
import { catalogOf, FakePlatform, fakeGlobe, layer, quakeSnapshot } from '../test/fakes';
import { FeedController } from './FeedController';

const catalog: Catalog = catalogOf([
  layer('earthquakes'),
  layer('aircraft'),
  layer('satellites', { availability: { state: 'disabledByLicence' } }),
]);

describe('FeedController', () => {
  let platform: FakePlatform;
  let stores: ReturnType<typeof createStores>;
  let controller: FeedController;

  beforeEach(() => {
    vi.useFakeTimers();
    platform = new FakePlatform();
    stores = createStores();
    stores.system.getState().setCatalog(catalog);
    controller = new FeedController(platform, stores);
    controller.start();
  });

  afterEach(() => {
    controller.stop();
    vi.useRealTimers();
  });

  it('subscribes when a layer is turned on and closes when it is turned off', () => {
    const { globe } = fakeGlobe();
    controller.attachGlobe(globe);
    stores.workspace.getState().setLayerEnabled('earthquakes', true);
    expect(platform.subs).toHaveLength(1);
    expect(stores.feeds.getState().summaries.earthquakes?.phase).toBe('connecting');
    stores.workspace.getState().setLayerEnabled('earthquakes', false);
    expect(platform.subs[0]?.closed).toBe(true);
    expect(globe.clearFeed).toHaveBeenCalledWith('earthquakes');
    expect(stores.feeds.getState().summaries.earthquakes).toBeUndefined();
  });

  it('never subscribes to a layer the catalog marks unavailable', () => {
    stores.workspace.getState().setLayerEnabled('satellites', true);
    expect(platform.subs).toHaveLength(0);
  });

  it('applies snapshots to the globe and summarises them without React', () => {
    const { globe } = fakeGlobe();
    controller.attachGlobe(globe);
    stores.workspace.getState().setLayerEnabled('earthquakes', true);
    const snapshot = quakeSnapshot(['a', 'b']);
    platform.subs[0]?.onEvent({ kind: 'snapshot', snapshot });
    expect(globe.applySnapshot).toHaveBeenCalledWith(snapshot);
    expect(stores.feeds.getState().summaries.earthquakes).toMatchObject({
      phase: 'ready',
      count: 2,
      sourceCount: 3,
    });
  });

  it('keeps the last summary and marks errors', () => {
    stores.workspace.getState().setLayerEnabled('earthquakes', true);
    platform.subs[0]?.onEvent({ kind: 'snapshot', snapshot: quakeSnapshot(['a']) });
    platform.subs[0]?.onEvent({
      kind: 'error',
      error: { code: 'upstreamUnavailable', message: 'down', retryable: true },
    });
    expect(stores.feeds.getState().summaries.earthquakes).toMatchObject({
      phase: 'error',
      count: 1,
      error: { message: 'down' },
    });
  });

  it('replays known snapshots to a globe attached later', () => {
    stores.workspace.getState().setLayerEnabled('earthquakes', true);
    const snapshot = quakeSnapshot(['a']);
    platform.subs[0]?.onEvent({ kind: 'snapshot', snapshot });
    const { globe } = fakeGlobe();
    controller.attachGlobe(globe);
    expect(globe.applySnapshot).toHaveBeenCalledWith(snapshot);
  });

  it('debounces viewport changes into subscription updates', () => {
    const { globe, view } = fakeGlobe();
    controller.attachGlobe(globe);
    stores.workspace.getState().setLayerEnabled('earthquakes', true);
    const bbox = { west: 20, south: 55, east: 30, north: 65 };
    view({ bbox, centre: { lat: 60, lon: 25 }, heightM: 1e6 });
    view({ bbox: { ...bbox, west: 21 }, centre: null, heightM: 1e6 });
    expect(platform.subs[0]?.updates).toHaveLength(0);
    vi.advanceTimersByTime(350);
    expect(platform.subs[0]?.updates).toEqual([
      { feed: 'earthquakes', bbox: { ...bbox, west: 21 }, filters: {} },
    ]);
  });

  it('sends filter changes as updates', () => {
    stores.workspace.getState().setLayerEnabled('earthquakes', true);
    stores.workspace.getState().setFilters('earthquakes', { minMagnitude: 4 });
    expect(platform.subs[0]?.updates.at(-1)?.filters).toEqual({ minMagnitude: 4 });
  });

  it('tracks the selected entity, and says when it leaves the feed', () => {
    const { globe, pick } = fakeGlobe();
    controller.attachGlobe(globe);
    stores.workspace.getState().setLayerEnabled('earthquakes', true);
    platform.subs[0]?.onEvent({ kind: 'snapshot', snapshot: quakeSnapshot(['a', 'b']) });
    pick({ feed: 'earthquakes', id: 'b' });
    expect(stores.workspace.getState().selection).toEqual({ feed: 'earthquakes', id: 'b' });
    expect(globe.setSelection).toHaveBeenLastCalledWith({ feed: 'earthquakes', id: 'b' });
    expect(stores.feeds.getState()).toMatchObject({
      selectedPresent: true,
      selectedEntity: { id: 'b' },
    });
    platform.subs[0]?.onEvent({ kind: 'snapshot', snapshot: quakeSnapshot(['a']) });
    expect(stores.feeds.getState()).toMatchObject({
      selectedPresent: false,
      selectedEntity: { id: 'b' },
    });
    pick({ feed: 'earthquakes', id: 'zzz' });
    expect(stores.feeds.getState().selectedEntity).toBeNull();
  });

  it('turning a layer off clears a selection on it', () => {
    stores.workspace.getState().setLayerEnabled('earthquakes', true);
    stores.workspace.getState().select({ feed: 'earthquakes', id: 'a' });
    stores.workspace.getState().setLayerEnabled('earthquakes', false);
    expect(stores.workspace.getState().selection).toBeNull();
  });

  it('requests an orbit for a selected satellite', async () => {
    const { globe } = fakeGlobe();
    controller.attachGlobe(globe);
    stores.workspace.getState().select({ feed: 'satellites', id: '25544' });
    await vi.runAllTimersAsync();
    expect(platform.tracks).toEqual([25544]);
    expect(globe.showTrack).toHaveBeenCalledWith(expect.objectContaining({ noradId: 25544 }));
  });
});
