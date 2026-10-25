import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { App } from '../../app/App';
import { type Harness, renderWithRuntime, startHarness } from '../../app/testing';
import { createRuntime } from '../../runtime';
import { WORKSPACE_PREF_KEY } from '../../runtime/workspacePrefs';
import { pin } from '../../test/annotation';
import { FakePlatform, fakeGlobe, memoryPreferences } from '../../test/fakes';
import { GLOBE_REGION_ID } from '../commands';
import { AnnotationsPanel } from './AnnotationsPanel';

const harnesses: Harness[] = [];
async function setup() {
  const h = await startHarness({ globe: true });
  harnesses.push(h);
  const view = renderWithRuntime(
    h.runtime,
    <>
      <main id={GLOBE_REGION_ID} tabIndex={-1} aria-label="Globe" />
      <AnnotationsPanel />
    </>,
  );
  return { h, view, user: userEvent.setup() };
}
afterEach(() => {
  for (const h of harnesses.splice(0)) h.stop();
});
describe('accessible annotation editor', () => {
  it('appends to eager device marks before a delayed catalog settles and keeps both after reload', async () => {
    const platform = new FakePlatform();
    platform.preferences = memoryPreferences({
      [WORKSPACE_PREF_KEY]: {
        version: 1,
        layers: { aircraft: { enabled: true, filters: {} } },
        imageryId: 'night',
        annotations: [pin],
      },
    });
    let finish: ((catalog: Exclude<typeof platform.catalogResult, Error>) => void) | undefined;
    platform.catalog = vi.fn(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const runtime = createRuntime(platform, { workspaceLocation: null });
    const stop = runtime.start();
    const view = renderWithRuntime(runtime, <AnnotationsPanel />);
    const user = userEvent.setup();
    expect(screen.getByRole('heading', { name: pin.label })).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'New pin' }));
    await user.type(screen.getByLabelText('Label', { exact: true }), 'New point');
    await user.type(screen.getByLabelText('Latitude', { exact: true }), '35');
    await user.type(screen.getByLabelText('Longitude', { exact: true }), '-95');
    await user.click(screen.getByRole('button', { name: 'Add point' }));
    await user.click(screen.getByRole('button', { name: 'Save annotation' }));
    expect(runtime.stores.workspace.getState().annotations).toHaveLength(2);
    expect(
      platform.preferences.get<{ layers: unknown; imageryId: unknown }>(WORKSPACE_PREF_KEY),
    ).toMatchObject({ layers: { aircraft: { enabled: true, filters: {} } }, imageryId: 'night' });
    const earlyReload = createRuntime(platform, { workspaceLocation: null });
    expect(earlyReload.stores.workspace.getState().annotations.map((a) => a.label)).toEqual([
      pin.label,
      'New point',
    ]);
    const catalog = platform.catalogResult;
    if (catalog instanceof Error) throw catalog;
    await act(async () => finish?.(catalog));
    expect(runtime.stores.workspace.getState().annotations).toHaveLength(2);
    view.unmount();
    stop();
    const reloaded = createRuntime(platform, { workspaceLocation: null });
    expect(reloaded.stores.workspace.getState().annotations.map((a) => a.label)).toEqual([
      pin.label,
      'New point',
    ]);
  });

  it('consumes drawing Escape before global selection shortcuts while menus and dialogs keep ownership', async () => {
    const h = await startHarness();
    harnesses.push(h);
    const fake = fakeGlobe();
    render(<App runtime={h.runtime} mountGlobe={async () => fake.globe} />);
    const user = userEvent.setup();
    await screen.findByRole('button', { name: 'Copy view link' });
    const selected = { feed: 'aircraft' as const, id: '461f2a' };
    act(() => h.runtime.stores.workspace.getState().select(selected));
    await user.click(screen.getByRole('button', { name: 'Annotations' }));
    await user.click(screen.getByRole('button', { name: 'New pin' }));
    screen.getByRole('main', { name: 'Globe' }).focus();
    await user.keyboard('{Escape}');
    expect(h.runtime.stores.workspace.getState().selection).toEqual(selected);
    expect(screen.queryByRole('button', { name: 'Cancel drawing' })).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'New pin' }));
    await user.click(screen.getByRole('button', { name: 'Imagery' }));
    expect(await screen.findByRole('menu')).toBeVisible();
    await user.keyboard('{Escape}');
    expect(screen.getByRole('button', { name: 'Cancel drawing' })).toBeVisible();
    act(() => h.runtime.stores.ui.getState().setShortcutsOpen(true));
    await waitFor(() => expect(screen.getByRole('dialog')).toBeVisible());
    await user.keyboard('{Escape}');
    expect(h.runtime.stores.ui.getState().shortcutsOpen).toBe(false);
    expect(screen.getByRole('button', { name: 'Cancel drawing' })).toBeVisible();
    expect(h.runtime.stores.workspace.getState().selection).toEqual(selected);
  });
  it('creates with keyboard coordinate entry, edits labels/notes/geometry, and deletes from the list', async () => {
    const { h, user } = await setup();
    await user.click(screen.getByRole('button', { name: 'New pin' }));
    await user.type(screen.getByLabelText('Label', { exact: true }), 'Meeting point');
    await user.type(screen.getByLabelText('Note', { exact: true }), 'North entrance');
    await user.type(screen.getByLabelText('Latitude', { exact: true }), '60.17');
    await user.type(screen.getByLabelText('Longitude', { exact: true }), '24.94');
    await user.click(screen.getByRole('button', { name: 'Add point' }));
    screen.getByRole('button', { name: 'Save annotation' }).focus();
    await user.keyboard('{Enter}');
    expect(screen.getByRole('heading', { name: 'Meeting point' })).toBeVisible();
    expect(screen.getByText('North entrance')).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Edit Meeting point' }));
    await user.clear(screen.getByLabelText('Note', { exact: true }));
    await user.type(screen.getByLabelText('Note', { exact: true }), 'South entrance');
    await user.clear(screen.getByLabelText(/Vertices/));
    await user.type(screen.getByLabelText(/Vertices/), '60.18, 24.95');
    await user.click(screen.getByRole('button', { name: 'Apply vertices' }));
    await user.click(screen.getByRole('button', { name: 'Save annotation' }));
    expect(h.runtime.stores.workspace.getState().annotations[0]).toMatchObject({
      note: 'South entrance',
      points: [{ lat: 60.18, lon: 24.95 }],
    });
    await user.click(screen.getByRole('button', { name: 'Delete Meeting point' }));
    expect(screen.getByText(/No annotations yet/)).toBeVisible();
  });

  it('ignores CSS-hidden popup DOM and leaves Enter inside an editable field to that field', async () => {
    const { h, user } = await setup();
    await user.click(screen.getByRole('button', { name: 'New pin' }));
    await user.type(screen.getByLabelText('Label', { exact: true }), 'Keyboard mark');
    act(() => h.globe?.point({ lat: 35, lon: -95 }));
    await user.keyboard('{Enter}');
    expect(h.runtime.stores.workspace.getState().annotations).toHaveLength(0);
    const hidden = document.createElement('div');
    hidden.style.display = 'none';
    hidden.innerHTML = '<div role="listbox"></div>';
    document.body.append(hidden);
    try {
      fireEvent.keyDown(screen.getByRole('main', { name: 'Globe' }), { key: 'Enter' });
      expect(h.runtime.stores.workspace.getState().annotations).toHaveLength(1);
      expect(screen.getByRole('heading', { name: 'Keyboard mark' })).toBeVisible();
    } finally {
      hidden.remove();
    }
  });
  it('leaves links and coordinate summaries their native keys while a valid draft exists', async () => {
    const h = await startHarness();
    harnesses.push(h);
    const fake = fakeGlobe();
    render(<App runtime={h.runtime} mountGlobe={async () => fake.globe} />);
    const user = userEvent.setup();
    await screen.findByRole('button', { name: 'Copy view link' });
    act(() => h.runtime.stores.workspace.getState().setAnnotations([pin]));
    await user.click(screen.getByRole('button', { name: 'Annotations' }));
    await user.click(screen.getByRole('button', { name: 'New pin' }));
    act(() => fake.point({ lat: 35, lon: -95 }));
    for (const target of [
      screen.getByRole('link', { name: 'Developer documentation' }),
      screen.getByText('1 coordinate'),
    ]) {
      target.focus();
      for (const key of ['Enter', 'Backspace']) {
        const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true });
        fireEvent(target, event);
        expect(event.defaultPrevented).toBe(false);
      }
      expect(h.runtime.stores.workspace.getState().annotations).toEqual([pin]);
      expect(screen.getByRole('button', { name: 'Save annotation' })).toBeEnabled();
    }
  });
  it('owns map clicks and supports Enter, Backspace, Escape with cleanup on unmount', async () => {
    const { h, user, view } = await setup();
    await user.click(screen.getByRole('button', { name: 'New line' }));
    act(() => {
      h.globe?.point({ lat: 0, lon: 179 });
      h.globe?.point({ lat: 0, lon: -179 });
    });
    expect(screen.getByRole('button', { name: 'Save annotation' })).toBeEnabled();
    fireEvent.keyDown(screen.getByRole('main', { name: 'Globe' }), { key: 'Backspace' });
    expect(screen.getByRole('button', { name: 'Save annotation' })).toBeDisabled();
    act(() => h.globe?.point({ lat: 0, lon: -179 }));
    fireEvent.keyDown(screen.getByRole('main', { name: 'Globe' }), { key: 'Enter' });
    expect(screen.getByRole('heading', { name: 'Untitled line' })).toBeVisible();
    expect(screen.getByText('Approximate 222.39 km')).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'New area' }));
    act(() => h.globe?.point({ lat: 1, lon: 1 }));
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(h.runtime.stores.workspace.getState().annotations).toHaveLength(1);
    expect(screen.queryByRole('button', { name: 'Cancel drawing' })).not.toBeInTheDocument();
    expect(h.globe?.globe.setDrawing).toHaveBeenLastCalledWith(false);
    await user.click(screen.getByRole('button', { name: 'New pin' }));
    view.unmount();
    expect(h.globe?.globe.setDrawing).toHaveBeenLastCalledWith(false);
  });
  it('reports bad coordinates and ignores duplicate clicks, while external restoration cancels a draft', async () => {
    const { h, user } = await setup();
    await user.click(screen.getByRole('button', { name: 'New line' }));
    act(() => h.globe?.point(null));
    expect(
      screen.getByRole('region', { name: 'Annotations' }).querySelector('[role=alert]'),
    ).toHaveTextContent(/Earth/);
    await user.type(screen.getByLabelText(/Vertices/), '91, 0');
    await user.click(screen.getByRole('button', { name: 'Apply vertices' }));
    expect(
      screen.getByRole('region', { name: 'Annotations' }).querySelector('[role=alert]'),
    ).toHaveTextContent(/Coordinates/);
    act(() => {
      h.globe?.point({ lat: 0, lon: 0 });
      h.globe?.point({ lat: 0, lon: 0 });
    });
    expect(
      screen.getByRole('region', { name: 'Annotations' }).querySelector('[role=alert]'),
    ).toHaveTextContent(/0.5 m/);
    act(() => {
      const state = h.runtime.stores.workspace.getState();
      state.restore(state.layers, state.imageryId, state.selection, [pin]);
    });
    expect(screen.queryByRole('button', { name: 'Cancel drawing' })).not.toBeInTheDocument();
    expect(screen.getByRole('heading', { name: pin.label })).toBeVisible();
  });

  it('imports GeoJSON features into workspace annotations and flies to the first imported feature', async () => {
    const { h, user } = await setup();
    const fileInput = screen.getByLabelText('Import GeoJSON file');
    const geojson = JSON.stringify({
      type: 'FeatureCollection',
      features: [
        {
          type: 'Feature',
          properties: { name: 'Imported Beacon', description: 'VHF relay' },
          geometry: { type: 'Point', coordinates: [24.7536, 59.437] },
        },
      ],
    });
    const file = new File([geojson], 'beacon.geojson', { type: 'application/geo+json' });
    await user.upload(fileInput, file);

    expect(await screen.findByRole('heading', { name: 'Imported Beacon' })).toBeVisible();
    expect(screen.getByText('VHF relay')).toBeVisible();
    expect(h.globe?.globe.flyTo).toHaveBeenCalledWith({
      kind: 'point',
      position: { lat: 59.437, lon: 24.7536 },
    });
  });
});
