import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { type Harness, renderWithRuntime, source, startHarness } from '../../app/testing';
import { catalogOf, layer } from '../../test/fakes';
import { vessel, vesselSnapshot } from '../../test/vessel';
import { useCommandContext, useGlobalShortcuts } from '../commands';
import { CommandPalette } from './CommandPalette';
import { SearchBar } from './SearchBar';

const CATALOG = catalogOf(
  [layer('vessels', { name: 'Vessels', family: 'maritime', sourceIds: ['aisstream'] })],
  { sources: [source('aisstream', { name: 'AISStream' })] },
);

function Workspace() {
  useGlobalShortcuts(useCommandContext());
  return (
    <>
      <SearchBar />
      <CommandPalette />
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
});

describe('Vessel search', () => {
  it('says in both search fields that ships can be searched', async () => {
    const { user } = await setup();
    const bar = screen.getByRole('combobox', { name: 'Search places, coordinates and callsigns' });
    // The placeholder is also the field's accessible description.
    expect(bar).toHaveAttribute('placeholder', expect.stringMatching(/ships/));
    await user.keyboard('{Meta>}k{/Meta}');
    const dialog = await screen.findByRole('dialog', { name: 'Search and commands' });
    expect(within(dialog).getByRole('combobox')).toHaveAttribute(
      'placeholder',
      expect.stringMatching(/ships/),
    );
  });

  it('lists a server vessel hit by MMSI, turns the layer on, selects it and flies there', async () => {
    const { user, ...h } = await setup();
    h.platform.searchResult = {
      coordinates: [],
      places: [],
      entities: [
        {
          feed: 'vessels',
          id: '230990002',
          label: 'TEST FERRY',
          detail: 'Passenger',
          position: { lat: 59.8, lon: 24.82 },
        },
      ],
      notices: [],
    };
    await user.keyboard('{Meta>}k{/Meta}');
    const dialog = await screen.findByRole('dialog', { name: 'Search and commands' });
    await user.type(within(dialog).getByRole('combobox'), '230990002');
    const hit = await within(dialog).findByRole('option', { name: /TEST FERRY/ });
    expect(hit).toHaveTextContent('Passenger');
    expect(hit).toHaveTextContent('Select and fly to');
    await user.click(hit);
    const ws = h.runtime.stores.workspace.getState();
    expect(ws.selection).toEqual({ feed: 'vessels', id: '230990002' });
    expect(ws.layers.vessels?.enabled).toBe(true);
    expect(h.globe?.globe.flyTo).toHaveBeenLastCalledWith({
      kind: 'point',
      position: { lat: 59.8, lon: 24.82 },
    });
    await waitFor(() => expect(h.globe?.globe.setSelection).toHaveBeenLastCalledWith(ws.selection));
  });

  it('finds loaded vessels by name and MMSI without the server and selects on Enter', async () => {
    const { user, ...h } = await setup();
    h.platform.search.mockRejectedValue(new Error('search down'));
    act(() => h.runtime.stores.workspace.getState().setLayerEnabled('vessels', true));
    act(() =>
      h.platform.live('vessels')?.onEvent({
        kind: 'snapshot',
        snapshot: vesselSnapshot([
          vessel({ mmsi: '230990001', name: 'TEST TRADER', vesselType: 'Cargo' }),
          vessel({ mmsi: '276990003', name: 'TEST TANKER', position: { lat: 59.5, lon: 24.7 } }),
        ]),
      }),
    );
    const field = screen.getByRole('combobox', {
      name: 'Search places, coordinates and callsigns',
    });
    await user.click(field);
    await user.type(field, 'test tra');
    const byName = screen.getByRole('option', { name: /TEST TRADER/ });
    expect(byName).toHaveTextContent('MMSI 230990001 · Cargo');
    expect(screen.queryByRole('option', { name: /TEST TANKER/ })).not.toBeInTheDocument();

    await user.clear(field);
    await user.type(field, '27699');
    expect(screen.getAllByRole('option')[0]).toHaveTextContent('TEST TANKER');
    await user.keyboard('{Enter}');
    expect(h.runtime.stores.workspace.getState().selection).toEqual({
      feed: 'vessels',
      id: '276990003',
    });
    expect(h.globe?.globe.flyTo).toHaveBeenLastCalledWith({
      kind: 'point',
      position: { lat: 59.5, lon: 24.7 },
    });
  });
});
