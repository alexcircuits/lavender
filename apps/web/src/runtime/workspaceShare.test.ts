import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { clearToasts, toastStore } from '../design';
import { workspaceShareUrl } from '../state/workspaceDocument';
import { catalogOf, FakePlatform, fakeGlobe, layer } from '../test/fakes';
import { document } from '../test/workspaceDocument';
import { createRuntime } from './createRuntime';
import type { WorkspaceLocation } from './workspaceShare';

const urlFor = (doc = document) => workspaceShareUrl(doc, 'https://lavender.example/');
function location(initial: string) {
  let url = initial;
  const listeners = new Set<() => void>();
  const port: WorkspaceLocation = {
    currentUrl: () => url,
    onNavigate: (listener) => {
      listeners.add(listener);
      return () => void listeners.delete(listener);
    },
  };
  return {
    port,
    navigate: (next: string) => {
      url = next;
      for (const listener of listeners) listener();
    },
    listeners,
  };
}

describe('workspace share restoration', () => {
  const stops: Array<() => void> = [];
  beforeEach(() => {
    vi.useFakeTimers();
    clearToasts();
  });
  afterEach(() => {
    for (const stop of stops.splice(0)) stop();
    clearToasts();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it.each(['unconfigured', 'disabledByLicence'] as const)(
    'clears shared selections when the feed is %s',
    async (availability) => {
      const platform = new FakePlatform();
      platform.catalogResult = catalogOf([
        layer('aircraft', {
          availability:
            availability === 'unconfigured'
              ? { state: availability, missing: ['key'] }
              : { state: availability },
        }),
      ]);
      const runtime = createRuntime(platform, { workspaceLocation: location(urlFor()).port });
      stops.push(runtime.start());
      await vi.advanceTimersByTimeAsync(0);
      expect(runtime.stores.workspace.getState().selection).toBeNull();
      expect(platform.live('aircraft')).toBeUndefined();
    },
  );

  it('restores registered layers, options, imagery and a selection once data can arrive', async () => {
    const platform = new FakePlatform();
    const shared = {
      ...document,
      layers: [...document.layers, { id: 'ships', enabled: true, options: {} }],
    };
    const browser = location(urlFor(shared));
    const runtime = createRuntime(platform, { workspaceLocation: browser.port });
    stops.push(runtime.start());
    const { globe } = fakeGlobe();
    runtime.attachGlobe(globe);
    expect(globe.restoreCamera).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(0);
    expect(runtime.stores.workspace.getState()).toMatchObject({
      imageryId: 'offline',
      selection: document.selection,
      layers: {
        aircraft: { enabled: true, filters: { militaryOnly: false } },
        earthquakes: { enabled: false },
      },
    });
    expect(Object.keys(runtime.stores.workspace.getState().layers)).not.toContain('ships');
    expect(platform.live('aircraft')).toBeDefined();
    expect(platform.live('earthquakes')).toBeUndefined();
    expect(globe.restoreCamera).toHaveBeenCalledExactlyOnceWith(document.camera);
    expect(globe.setSelection).toHaveBeenLastCalledWith(document.selection);
  });

  it('preserves user intent in every lane while a catalog response is delayed', async () => {
    const platform = new FakePlatform();
    let finish: ((catalog: ReturnType<typeof catalogOf>) => void) | undefined;
    platform.catalog = vi.fn(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const runtime = createRuntime(platform, { workspaceLocation: location(urlFor()).port });
    stops.push(runtime.start());
    const fake = fakeGlobe();
    runtime.attachGlobe(fake.globe);
    const workspace = runtime.stores.workspace.getState();
    workspace.setLayerEnabled('aircraft', false);
    workspace.setFilters('earthquakes', { minMagnitude: 6 });
    workspace.setImagery('night');
    workspace.select(null); // A no-op still expresses newer intent.
    fake.globe.home();
    finish?.(platform.catalogResult as ReturnType<typeof catalogOf>);
    await vi.advanceTimersByTimeAsync(0);
    expect(runtime.stores.workspace.getState()).toMatchObject({
      selection: null,
      imageryId: 'night',
      layers: { aircraft: { enabled: false }, earthquakes: { filters: { minMagnitude: 6 } } },
    });
    expect(fake.globe.restoreCamera).not.toHaveBeenCalled();
  });

  it('camera pointer/wheel intent claims its lane without blocking workspace restoration', async () => {
    const platform = new FakePlatform();
    const runtime = createRuntime(platform, { workspaceLocation: location(urlFor()).port });
    stops.push(runtime.start());
    const fake = fakeGlobe();
    runtime.attachGlobe(fake.globe);
    fake.cameraIntent();
    await vi.advanceTimersByTimeAsync(0);
    expect(runtime.stores.workspace.getState().layers.aircraft?.enabled).toBe(true);
    expect(fake.globe.restoreCamera).not.toHaveBeenCalled();
  });

  it('preserves a newer selection while restoring the other workspace lanes', async () => {
    const runtime = createRuntime(new FakePlatform(), {
      workspaceLocation: location(urlFor()).port,
    });
    stops.push(runtime.start());
    const newer = { feed: 'satellites' as const, id: '25544' };
    runtime.stores.workspace.getState().select(newer);
    await vi.advanceTimersByTimeAsync(0);
    expect(runtime.stores.workspace.getState().selection).toEqual(newer);
    expect(runtime.stores.workspace.getState().layers.aircraft?.enabled).toBe(true);
    const { globe } = fakeGlobe();
    runtime.attachGlobe(globe);
    expect(globe.setSelection).toHaveBeenLastCalledWith(newer);
    expect(globe.restoreCamera).toHaveBeenCalledExactlyOnceWith(document.camera);
  });

  it('never replaces workspace choices with a stale aborted catalog response', async () => {
    const platform = new FakePlatform();
    const finishes: Array<(catalog: ReturnType<typeof catalogOf>) => void> = [];
    platform.catalog = vi.fn(
      () =>
        new Promise((resolve) => {
          finishes.push(resolve);
        }),
    );
    const runtime = createRuntime(platform, { workspaceLocation: location(urlFor()).port });
    stops.push(runtime.start());
    runtime.reloadCatalog();
    finishes[1]?.(platform.catalogResult as ReturnType<typeof catalogOf>);
    await vi.advanceTimersByTimeAsync(0);
    runtime.stores.workspace.getState().setLayerEnabled('aircraft', false);
    finishes[0]?.(catalogOf([layer('earthquakes', { defaultOn: true })]));
    await vi.advanceTimersByTimeAsync(0);
    expect(runtime.stores.system.getState().catalog).toBe(platform.catalogResult);
    expect(runtime.stores.workspace.getState().layers.aircraft?.enabled).toBe(false);
  });

  it('uses the latest navigation before catalog/globe readiness and removes navigation listeners on stop', async () => {
    const platform = new FakePlatform();
    const browser = location(urlFor());
    const runtime = createRuntime(platform, { workspaceLocation: browser.port });
    const stop = runtime.start();
    stops.push(stop);
    const second = {
      ...document,
      camera: { ...document.camera, lon: -120 },
      selection: null,
      layers: [{ id: 'earthquakes', enabled: true, options: { minMagnitude: 5 } }],
    };
    browser.navigate(urlFor(second));
    await vi.advanceTimersByTimeAsync(0);
    const { globe } = fakeGlobe();
    runtime.attachGlobe(globe);
    expect(globe.restoreCamera).toHaveBeenCalledExactlyOnceWith(second.camera);
    expect(platform.live('aircraft')).toBeUndefined();
    expect(platform.live('earthquakes')?.query.filters).toEqual({ minMagnitude: 5 });
    browser.navigate(urlFor());
    expect(globe.restoreCamera).toHaveBeenLastCalledWith(document.camera);
    expect(runtime.stores.workspace.getState().selection).toEqual(document.selection);
    stop();
    expect(browser.listeners.size).toBe(0);
  });

  it('rejects malformed navigation with a short toast and cancels the earlier pending restore', async () => {
    const browser = location(urlFor());
    const runtime = createRuntime(new FakePlatform(), { workspaceLocation: browser.port });
    stops.push(runtime.start());
    browser.navigate('https://lavender.example/#view=broken!');
    await vi.advanceTimersByTimeAsync(0);
    const { globe } = fakeGlobe();
    runtime.attachGlobe(globe);
    expect(globe.restoreCamera).not.toHaveBeenCalled();
    expect(toastStore.getState().toasts).toEqual([
      expect.objectContaining({ title: 'Could not open shared view', kind: 'error' }),
    ]);
  });

  it('copies a captured view with feedback and reports clipboard failure', async () => {
    const writeText = vi.fn(async (_text: string) => {});
    vi.stubGlobal('navigator', { clipboard: { writeText } });
    const runtime = createRuntime(new FakePlatform(), {
      workspaceLocation: location('https://lavender.example/').port,
    });
    stops.push(runtime.start());
    runtime.attachGlobe(fakeGlobe().globe);
    await vi.advanceTimersByTimeAsync(0);
    await runtime.copyViewLink();
    expect(writeText).toHaveBeenCalledWith(expect.stringContaining('#view='));
    expect(toastStore.getState().toasts.at(-1)?.title).toBe('Link copied');
    writeText.mockRejectedValueOnce(new Error('denied'));
    await runtime.copyViewLink();
    expect(toastStore.getState().toasts.at(-1)?.title).toBe('Could not copy link');
  });

  it('drops unknown imagery and selections from unknown or disabled layers', async () => {
    const platform = new FakePlatform();
    platform.catalogResult = catalogOf([layer('earthquakes')]);
    const runtime = createRuntime(platform, {
      workspaceLocation: location(urlFor({ ...document, imageryId: 'missing' })).port,
    });
    stops.push(runtime.start());
    await vi.advanceTimersByTimeAsync(0);
    expect(runtime.stores.workspace.getState()).toMatchObject({ selection: null, imageryId: null });
  });
});
