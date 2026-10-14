import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  aircraft,
  aircraftSnapshot,
  type Harness,
  renderWithRuntime,
  source,
  startHarness,
} from '../../app/testing';
import type { SearchResults } from '../../contracts';
import { Button } from '../../design';
import { catalogOf, layer } from '../../test/fakes';
import { ShortcutSheet, useCommandContext, useGlobalShortcuts } from '../commands';
import { CommandPalette } from './CommandPalette';
import { SearchBar } from './SearchBar';

const CATALOG = catalogOf(
  [
    layer('earthquakes', { name: 'Earthquakes' }),
    layer('aircraft', { name: 'Aircraft' }),
    layer('satellites', { name: 'Satellites', availability: { state: 'disabledByLicence' } }),
  ],
  { sources: [source('geonames', { name: 'GeoNames', attribution: '© GeoNames, CC BY 4.0' })] },
);

function Workspace() {
  useGlobalShortcuts(useCommandContext());
  return (
    <>
      <Button>Before</Button>
      <SearchBar />
      <CommandPalette />
      <ShortcutSheet />
    </>
  );
}

let harness: Harness | null = null;

async function setup() {
  harness = await startHarness({ catalog: CATALOG, globe: true });
  renderWithRuntime(harness.runtime, <Workspace />);
  return { ...harness, user: userEvent.setup() };
}

afterEach(() => {
  harness?.stop();
  harness = null;
  vi.restoreAllMocks();
});

async function openPalette(user: ReturnType<typeof userEvent.setup>) {
  await user.keyboard('{Meta>}k{/Meta}');
  return screen.findByRole('dialog', { name: 'Search and commands' });
}

const selected = (dialog: HTMLElement) =>
  within(dialog)
    .getAllByRole('option')
    .find((o) => o.getAttribute('aria-selected') === 'true');

describe('Command palette', () => {
  it('retries a failed explicit place search without editing the query', async () => {
    const { user, ...h } = await setup();
    let attempts = 0;
    h.platform.search.mockImplementation(async (query) => {
      h.platform.searches.push(query);
      if (!query.places) return { coordinates: [], places: [], entities: [], notices: [] };
      attempts += 1;
      if (attempts === 1) throw new Error('temporarily unavailable');
      return {
        coordinates: [],
        places: [{ name: 'Helsinki', position: { lat: 60.17, lon: 24.94 }, sourceId: 'geonames' }],
        entities: [],
        notices: [],
      };
    });
    const dialog = await openPalette(user);
    await user.type(within(dialog).getByRole('combobox'), 'Helsinki');
    await user.click(within(dialog).getByRole('option', { name: /Search places for/ }));
    const retry = await within(dialog).findByRole('option', { name: /Search places again for/ });
    await user.click(retry);
    expect(await within(dialog).findByRole('option', { name: /Helsinki/ })).toBeVisible();
    expect(attempts).toBe(2);
  });

  it('ignores an old submitted place response after the text changes', async () => {
    const { user, ...h } = await setup();
    let finish: ((result: SearchResults) => void) | undefined;
    let oldSignal: AbortSignal | undefined;
    h.platform.search.mockImplementation(async (query, options) => {
      if (query.places) {
        oldSignal = options?.signal;
        return new Promise((resolve) => {
          finish = resolve;
        });
      }
      return { coordinates: [], places: [], entities: [], notices: [] };
    });
    const dialog = await openPalette(user);
    const input = within(dialog).getByRole('combobox');
    await user.type(input, 'Helsinki');
    await user.click(within(dialog).getByRole('option', { name: /Search places for/ }));
    await waitFor(() => expect(finish).toBeDefined());
    await user.clear(input);
    await user.type(input, 'Berlin');
    expect(oldSignal?.aborted).toBe(true);
    act(() =>
      finish?.({
        coordinates: [],
        places: [
          {
            name: 'Old Helsinki result',
            position: { lat: 60.17, lon: 24.94 },
            sourceId: 'geonames',
          },
        ],
        entities: [],
        notices: [],
      }),
    );
    await waitFor(() =>
      expect(
        within(dialog).queryByRole('option', { name: /Old Helsinki result/ }),
      ).not.toBeInTheDocument(),
    );
  });
  it('opens with ⌘K, flies to typed coordinates on Enter, and returns focus', async () => {
    const { user, ...h } = await setup();
    const before = screen.getByRole('button', { name: 'Before' });
    before.focus();

    const dialog = await openPalette(user);
    const input = within(dialog).getByRole('combobox');
    await waitFor(() => expect(input).toHaveFocus());
    expect(h.runtime.stores.ui.getState().paletteOpen).toBe(true);

    await user.type(input, '60.17, 24.94');
    const options = within(dialog).getAllByRole('option');
    expect(options[0]).toHaveTextContent('Go to 60.1700° N, 24.9400° E');
    await waitFor(() => expect(selected(dialog)).toBe(options[0]));

    await user.keyboard('{Enter}');
    expect(h.globe?.globe.flyTo).toHaveBeenCalledWith({
      kind: 'point',
      position: { lat: 60.17, lon: 24.94 },
    });
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    await waitFor(() => expect(before).toHaveFocus());
  });

  it('recognises DMS and MGRS without the network', async () => {
    const { user, ...h } = await setup();
    const dialog = await openPalette(user);
    const input = within(dialog).getByRole('combobox');
    await user.type(input, '18SUJ2339007393');
    expect(within(dialog).getAllByRole('option')[0]).toHaveTextContent(
      'Go to 38.8977° N, 77.0365° W',
    );
    await user.clear(input);
    await user.type(input, `60°10'12"N 24°56'18"E`);
    expect(within(dialog).getAllByRole('option')[0]).toHaveTextContent(
      'Go to 60.1700° N, 24.9383° E',
    );
    // Typing alone never asks the server about coordinates before the debounce.
    expect(h.platform.search).not.toHaveBeenCalledWith(
      expect.objectContaining({ q: '1' }),
      expect.anything(),
    );
  });

  it('moves with the arrow keys and closes with Escape, returning focus', async () => {
    const { user } = await setup();
    const before = screen.getByRole('button', { name: 'Before' });
    before.focus();
    const dialog = await openPalette(user);
    // Empty query: layers and actions from the registry.
    expect(within(dialog).getByRole('option', { name: /Turn on Earthquakes/ })).toBeVisible();
    expect(within(dialog).getByRole('option', { name: /Reset north/ })).toBeVisible();
    const options = within(dialog).getAllByRole('option');
    await waitFor(() => expect(selected(dialog)).toBe(options[0]));
    await user.keyboard('{ArrowDown}');
    expect(selected(dialog)).toBe(options[1]);
    await user.keyboard('{ArrowUp}');
    expect(selected(dialog)).toBe(options[0]);

    await user.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    await waitFor(() => expect(before).toHaveFocus());
  });

  it('runs registry actions and turns layers on', async () => {
    const { user, ...h } = await setup();
    let dialog = await openPalette(user);
    await user.type(within(dialog).getByRole('combobox'), 'north');
    await user.keyboard('{Enter}');
    expect(h.globe?.globe.resetNorth).toHaveBeenCalledOnce();

    dialog = await openPalette(user);
    await user.type(within(dialog).getByRole('combobox'), 'aircr');
    await user.click(within(dialog).getByRole('option', { name: /Turn on Aircraft/ }));
    expect(h.runtime.stores.workspace.getState().layers.aircraft?.enabled).toBe(true);

    dialog = await openPalette(user);
    await user.type(within(dialog).getByRole('combobox'), 'satel');
    expect(within(dialog).getByRole('option', { name: /Turn on Satellites/ })).toHaveAttribute(
      'aria-disabled',
      'true',
    );
  });

  it('submits place lookup explicitly while live-entity search remains debounced', async () => {
    const { user, ...h } = await setup();
    const results: SearchResults = {
      coordinates: [],
      places: [
        {
          name: 'Helsinki',
          context: 'Uusimaa, Finland',
          kind: 'city',
          position: { lat: 60.17, lon: 24.94 },
          bbox: { west: 24.8, south: 60.1, east: 25.1, north: 60.3 },
          sourceId: 'geonames',
        },
      ],
      entities: [
        {
          feed: 'aircraft',
          id: '461f2a',
          label: 'FIN7LY',
          detail: 'A359',
          position: { lat: 60.3, lon: 24.9 },
        },
      ],
      notices: ['Place search is limited to 8 results.'],
    };
    h.platform.searchResult = results;
    h.platform.search.mockImplementation(async (query) => {
      h.platform.searches.push(query);
      return query.places ? results : { ...results, places: [], notices: [] };
    });
    const dialog = await openPalette(user);
    await user.type(within(dialog).getByRole('combobox'), 'hel');
    await waitFor(() => expect(h.platform.search).toHaveBeenCalledTimes(1));
    expect(h.platform.searches[0]?.places).toBe(false);
    expect(within(dialog).queryByRole('option', { name: /Helsinki/ })).not.toBeInTheDocument();
    await user.click(within(dialog).getByRole('option', { name: /Search places for/ }));
    expect(await within(dialog).findByRole('option', { name: /Helsinki/ })).toBeVisible();
    expect(h.platform.search).toHaveBeenCalledTimes(2);
    expect(h.platform.searches[1]?.places).toBe(true);
    expect(h.platform.searches[0]?.q).toBe('hel');
    expect(within(dialog).getByText('© GeoNames, CC BY 4.0')).toBeInTheDocument();
    expect(within(dialog).getByText('Place search is limited to 8 results.')).toBeInTheDocument();

    await user.click(within(dialog).getByRole('option', { name: /Helsinki/ }));
    expect(h.globe?.globe.flyTo).toHaveBeenCalledWith({
      kind: 'bbox',
      bbox: { west: 24.8, south: 60.1, east: 25.1, north: 60.3 },
    });

    const again = await openPalette(user);
    await user.type(within(again).getByRole('combobox'), 'fin7');
    await user.click(await within(again).findByRole('option', { name: /FIN7LY/ }));
    const ws = h.runtime.stores.workspace.getState();
    expect(ws.selection).toEqual({ feed: 'aircraft', id: '461f2a' });
    // Selecting an entity in a layer that was off turns the layer on.
    expect(ws.layers.aircraft?.enabled).toBe(true);
    expect(h.globe?.globe.flyTo).toHaveBeenLastCalledWith({
      kind: 'point',
      position: { lat: 60.3, lon: 24.9 },
    });
  });

  it('finds live entities locally when the server search fails, and says so', async () => {
    const { user, ...h } = await setup();
    act(() => h.runtime.stores.workspace.getState().setLayerEnabled('aircraft', true));
    act(() => {
      h.platform.live('aircraft')?.onEvent({
        kind: 'snapshot',
        snapshot: aircraftSnapshot([aircraft('4aca2d', { callsign: 'SAS1462', typeCode: 'A20N' })]),
      });
    });
    h.platform.search.mockRejectedValue(new Error('geocoder down'));
    const dialog = await openPalette(user);
    await user.type(within(dialog).getByRole('combobox'), 'sas14');
    expect(within(dialog).getByRole('option', { name: /SAS1462/ })).toBeVisible();
    expect(
      await within(dialog).findByText(
        'Place search is unavailable. Coordinates and live entities still work.',
      ),
    ).toBeInTheDocument();
  });

  it('updates live-entity matches as data arrives', async () => {
    const { user, ...h } = await setup();
    act(() => h.runtime.stores.workspace.getState().setLayerEnabled('aircraft', true));
    const dialog = await openPalette(user);
    await user.type(within(dialog).getByRole('combobox'), 'sas14');
    expect(within(dialog).queryByRole('option', { name: /SAS1462/ })).not.toBeInTheDocument();
    act(() => {
      h.platform.live('aircraft')?.onEvent({
        kind: 'snapshot',
        snapshot: aircraftSnapshot([aircraft('4aca2d', { callsign: 'SAS1462' })]),
      });
    });
    expect(within(dialog).getByRole('option', { name: /SAS1462/ })).toBeVisible();
  });

  it('says plainly when nothing matches', async () => {
    const { user } = await setup();
    const dialog = await openPalette(user);
    await user.type(within(dialog).getByRole('combobox'), 'xyzzy');
    await user.click(within(dialog).getByRole('option', { name: /Search places for/ }));
    expect(
      await within(dialog).findByText(
        "No matches for 'xyzzy'. Try a place name, coordinates or a callsign.",
      ),
    ).toBeInTheDocument();
  });
});

describe('Search field', () => {
  it('focuses with /, shows results inline and runs the first on Enter', async () => {
    const { user, ...h } = await setup();
    screen.getByRole('button', { name: 'Before' }).focus();
    await user.keyboard('/');
    const field = screen.getByRole('combobox', {
      name: 'Search places, coordinates and callsigns',
    });
    expect(field).toHaveFocus();
    expect(field).toHaveValue('');

    await user.type(field, '-33.8568 151.2153');
    expect(screen.getAllByRole('option')[0]).toHaveTextContent('Go to 33.8568° S, 151.2153° E');
    await user.keyboard('{Enter}');
    expect(h.globe?.globe.flyTo).toHaveBeenCalledWith({
      kind: 'point',
      position: { lat: -33.8568, lon: 151.2153 },
    });
    expect(field).toHaveValue('');

    await user.type(field, 'abc');
    await user.keyboard('{Escape}');
    expect(field).toHaveValue('');
    await user.keyboard('{Escape}');
    expect(field).not.toHaveFocus();
  });
});

describe('Shortcut sheet', () => {
  it('opens with ? and lists the registry shortcuts', async () => {
    const { user } = await setup();
    screen.getByRole('button', { name: 'Before' }).focus();
    await user.keyboard('?');
    const sheet = await screen.findByRole('dialog', { name: 'Keyboard shortcuts' });
    for (const title of ['Zoom in', 'Reset north', 'Home', 'Show keyboard shortcuts']) {
      expect(within(sheet).getByText(title)).toBeInTheDocument();
    }
    expect(within(sheet).getByText('Esc')).toBeInTheDocument();
  });

  it('closes with a single Escape and returns focus', async () => {
    const { user, ...h } = await setup();
    const before = screen.getByRole('button', { name: 'Before' });
    before.focus();
    await user.keyboard('?');
    await screen.findByRole('dialog', { name: 'Keyboard shortcuts' });
    await waitFor(() =>
      expect(screen.getByRole('dialog').contains(document.activeElement)).toBe(true),
    );
    await user.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(h.runtime.stores.ui.getState().shortcutsOpen).toBe(false);
    await waitFor(() => expect(before).toHaveFocus());
  });
});
