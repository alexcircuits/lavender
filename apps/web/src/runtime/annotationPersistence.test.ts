import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { workspaceShareUrl } from '../state/workspaceDocument';
import { pin } from '../test/annotation';
import { FakePlatform, fakeGlobe, memoryPreferences } from '../test/fakes';
import { document } from '../test/workspaceDocument';
import { createRuntime } from './createRuntime';
import { readWorkspace, WORKSPACE_PREF_KEY } from './workspacePrefs';

describe('durable annotation state', () => {
  const stops: Array<() => void> = [];
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    for (const stop of stops.splice(0)) stop();
    vi.useRealTimers();
  });
  it('writes before reload can cancel a debounce and restores identical marks in a new runtime', async () => {
    const platform = new FakePlatform();
    const runtime = createRuntime(platform, { workspaceLocation: null, persistDelayMs: 500 });
    const stop = runtime.start();
    await vi.advanceTimersByTimeAsync(0);
    runtime.stores.workspace.getState().setAnnotations([pin]);
    stop();
    const reloaded = createRuntime(platform, { workspaceLocation: null });
    const globe = fakeGlobe();
    reloaded.attachGlobe(globe.globe);
    stops.push(reloaded.start());
    await vi.advanceTimersByTimeAsync(0);
    expect(reloaded.stores.workspace.getState().annotations).toEqual([pin]);
    expect(globe.globe.setAnnotations).toHaveBeenLastCalledWith([pin], null);
  });
  it('preserves newer edit intent while catalog and link restoration are pending', async () => {
    const platform = new FakePlatform();
    let finish: ((catalog: typeof platform.catalogResult) => void) | undefined;
    platform.catalog = vi.fn(
      () =>
        new Promise((resolve) => {
          finish = resolve as typeof finish;
        }),
    );
    platform.preferences = memoryPreferences({
      [WORKSPACE_PREF_KEY]: { version: 1, layers: {}, imageryId: null, annotations: [pin] },
    });
    const runtime = createRuntime(platform, {
      workspaceLocation: {
        currentUrl: () =>
          workspaceShareUrl({ ...document, annotations: [pin] }, 'https://lavender.example'),
        onNavigate: () => () => {},
      },
    });
    stops.push(runtime.start());
    const newer = { ...pin, id: 'newer', label: 'New intent' };
    runtime.stores.workspace.getState().setAnnotations([newer]);
    finish?.(platform.catalogResult);
    await vi.advanceTimersByTimeAsync(0);
    expect(runtime.stores.workspace.getState().annotations).toEqual([newer]);
  });
  it('restores all shared marks and keeps current marks when a later link is invalid', async () => {
    const platform = new FakePlatform();
    const runtime = createRuntime(platform, { workspaceLocation: null });
    stops.push(runtime.start());
    runtime.attachGlobe(fakeGlobe().globe);
    await vi.advanceTimersByTimeAsync(0);
    expect(
      runtime.restoreViewUrl(
        workspaceShareUrl({ ...document, annotations: [pin] }, 'https://lavender.example'),
      ),
    ).toBe(true);
    expect(runtime.stores.workspace.getState().annotations).toEqual([pin]);
    expect(runtime.restoreViewUrl('https://lavender.example/#view=broken!')).toBe(false);
    expect(runtime.stores.workspace.getState().annotations).toEqual([pin]);
    runtime.reloadCatalog();
    await vi.advanceTimersByTimeAsync(0);
    expect(runtime.stores.workspace.getState().annotations).toEqual([pin]);
  });
  it('reports blocked device storage while keeping marks editable in the session', async () => {
    const platform = new FakePlatform();
    platform.preferences.set = () => {};
    const runtime = createRuntime(platform, { workspaceLocation: null });
    stops.push(runtime.start());
    await vi.advanceTimersByTimeAsync(0);
    runtime.stores.workspace.getState().setAnnotations([pin]);
    expect(runtime.stores.workspace.getState().annotationStorageError).toMatch(
      /only for this session/,
    );
    expect(runtime.stores.workspace.getState().annotations).toEqual([pin]);
  });

  it('rejects corrupt device marks, while old preferences without annotations still work', () => {
    const platform = new FakePlatform();
    const catalog = platform.catalogResult;
    if (catalog instanceof Error) throw catalog;
    expect(
      readWorkspace(
        { version: 1, layers: {}, annotations: [{ ...pin, approximate: false }] },
        catalog,
      ).annotations,
    ).toEqual([]);
    expect(readWorkspace({ version: 1, layers: {} }, catalog).annotations).toEqual([]);
  });
});
