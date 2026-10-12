import { act, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Profiler, type ProfilerOnRenderCallback } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  aircraft,
  aircraftSnapshot,
  type Harness,
  renderWithRuntime,
  source,
  startHarness,
} from '../../app/testing';
import { formatCount, List } from '../../design';
import { catalogOf, layer, quakeSnapshot } from '../../test/fakes';
import { LayerAnnouncer } from './LayerAnnouncer';
import { LayerExplorer } from './LayerExplorer';
import { LayerRow } from './LayerRow';

const CATALOG = catalogOf(
  [
    layer('earthquakes', { name: 'Earthquakes', sourceIds: ['usgs'] }),
    layer('aircraft', { name: 'Aircraft', sourceIds: ['adsb-lol'] }),
    layer('satellites', { name: 'Satellites', sourceIds: ['celestrak'] }),
  ],
  { sources: [source('usgs', { name: 'USGS' }), source('adsb-lol', { name: 'adsb.lol' })] },
);

let harness: Harness | null = null;

async function setup(catalog = CATALOG) {
  harness = await startHarness({ catalog });
  const view = renderWithRuntime(
    harness.runtime,
    <>
      <LayerExplorer catalog={catalog} />
      <LayerAnnouncer />
    </>,
  );
  return { ...harness, view, user: userEvent.setup() };
}

afterEach(() => {
  harness?.stop();
  harness = null;
  vi.restoreAllMocks();
});

function row(name: string): HTMLElement {
  const toggle = screen.getByRole('switch', { name });
  const li = toggle.closest('li');
  if (!li) throw new Error(`No row for ${name}`);
  return li;
}

describe('LayerExplorer', () => {
  it('groups layers by family in the explorer order', async () => {
    await setup();
    const groups = screen.getAllByRole('list').map((l) => l.getAttribute('aria-label'));
    expect(groups).toEqual(['Air layers', 'Space layers', 'Hazards layers']);
    expect(within(row('Earthquakes')).getByText('USGS')).toBeInTheDocument();
  });

  it('shows Connecting… until the first snapshot, then the count and freshness', async () => {
    const { user, ...h } = await setup();
    await user.click(screen.getByRole('switch', { name: 'Earthquakes' }));
    expect(h.runtime.stores.workspace.getState().layers.earthquakes?.enabled).toBe(true);
    expect(within(row('Earthquakes')).getByText('Connecting…')).toBeInTheDocument();

    act(() => {
      h.platform.live('earthquakes')?.onEvent({
        kind: 'snapshot',
        snapshot: quakeSnapshot(['a', 'b', 'c'], { fetchedAt: new Date().toISOString() }),
      });
    });
    const earthquakes = row('Earthquakes');
    expect(within(earthquakes).queryByText('Connecting…')).not.toBeInTheDocument();
    expect(within(earthquakes).getByText('3')).toBeInTheDocument();
    expect(within(earthquakes).getByText('events')).toBeInTheDocument();
    expect(within(earthquakes).getByRole('meter', { name: 'Earthquakes freshness' })).toBeVisible();
    expect(await screen.findByText('Earthquakes: 3 events')).toBeInTheDocument();
  });

  it('says why a layer is stale or unavailable, in full', async () => {
    const { user, ...h } = await setup();
    await user.click(screen.getByRole('switch', { name: 'Earthquakes' }));
    act(() => {
      h.platform.live('earthquakes')?.onEvent({
        kind: 'snapshot',
        snapshot: quakeSnapshot(['a'], { freshness: { state: 'stale', ageS: 240 } }),
      });
    });
    expect(within(row('Earthquakes')).getByText('Stale')).toBeInTheDocument();
    expect(within(row('Earthquakes')).getByText('4 min')).toBeInTheDocument();

    act(() => {
      h.platform.live('earthquakes')?.onEvent({
        kind: 'error',
        error: { code: 'upstreamUnavailable', message: 'USGS did not answer.', retryable: true },
      });
    });
    expect(within(row('Earthquakes')).getByText('Unavailable')).toBeInTheDocument();
    expect(within(row('Earthquakes')).getByText('USGS did not answer.')).toBeInTheDocument();
    // The last good count is kept, not replaced by zero.
    expect(within(row('Earthquakes')).getByText('1')).toBeInTheDocument();
  });

  it('disables unconfigured and licence-disabled layers and says why', async () => {
    const catalog = catalogOf([
      layer('earthquakes', { name: 'Earthquakes' }),
      layer('aircraft', {
        name: 'Aircraft',
        availability: { state: 'unconfigured', missing: ['ADSB_API_KEY'] },
      }),
      layer('satellites', { name: 'Satellites', availability: { state: 'disabledByLicence' } }),
    ]);
    const { user, ...h } = await setup(catalog);

    const aircraftSwitch = screen.getByRole('switch', { name: 'Aircraft' });
    expect(aircraftSwitch).toHaveAttribute('aria-disabled', 'true');
    expect(aircraftSwitch).toHaveAccessibleDescription('Needs ADSB_API_KEY');
    expect(within(row('Aircraft')).getByText('Not configured')).toBeInTheDocument();
    expect(
      within(row('Aircraft')).queryByRole('button', { name: 'Aircraft options' }),
    ).not.toBeInTheDocument();

    const satellitesSwitch = screen.getByRole('switch', { name: 'Satellites' });
    expect(satellitesSwitch).toHaveAttribute('aria-disabled', 'true');
    expect(satellitesSwitch).toHaveAccessibleDescription('Off under the commercial licence policy');
    expect(within(row('Satellites')).getByText('Licence')).toBeInTheDocument();

    await user.click(aircraftSwitch);
    expect(h.runtime.stores.workspace.getState().layers.aircraft?.enabled).toBeUndefined();

    // A not-configured row links to the Sources panel.
    await user.click(within(row('Aircraft')).getByRole('button', { name: 'Set up in Sources' }));
    expect(h.runtime.stores.ui.getState().sidePanel).toBe('sources');
    expect(h.runtime.stores.workspace.getState().layers.aircraft).toBeUndefined();
  });

  it('narrows rows with the filter and says when nothing matches', async () => {
    const { user } = await setup();
    const filter = screen.getByRole('searchbox', { name: 'Filter layers' });
    await user.type(filter, 'quake');
    expect(screen.getAllByRole('switch').map((s) => s.getAttribute('aria-label'))).toEqual([
      'Earthquakes',
    ]);
    await user.clear(filter);
    await user.type(filter, 'zzz');
    expect(screen.queryAllByRole('switch')).toEqual([]);
    expect(screen.getByText("No layers match 'zzz'.")).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Clear filter' }));
    expect(screen.getAllByRole('switch')).toHaveLength(3);
    expect(filter).toHaveFocus();
  });

  it('turns options into feed filters', async () => {
    const { user, ...h } = await setup();
    const workspace = () => h.runtime.stores.workspace.getState();
    await user.click(screen.getByRole('switch', { name: 'Earthquakes' }));

    const disclosure = screen.getByRole('button', { name: 'Earthquakes options' });
    expect(disclosure).toHaveAttribute('aria-expanded', 'false');
    await user.click(disclosure);
    expect(disclosure).toHaveAttribute('aria-expanded', 'true');
    await user.click(screen.getByRole('radio', { name: '4.5+' }));
    expect(workspace().layers.earthquakes?.filters).toEqual({ minMagnitude: 4.5 });
    expect(h.platform.live('earthquakes')?.updates.at(-1)?.filters).toEqual({ minMagnitude: 4.5 });
    await user.click(screen.getByRole('radio', { name: 'All' }));
    expect(workspace().layers.earthquakes?.filters).toEqual({});

    await user.click(screen.getByRole('button', { name: 'Aircraft options' }));
    await user.click(screen.getByRole('switch', { name: 'Military only' }));
    expect(workspace().layers.aircraft?.filters).toEqual({ militaryOnly: true });

    await user.click(screen.getByRole('button', { name: 'Satellites options' }));
    await user.click(screen.getByRole('switch', { name: 'GPS' }));
    expect(workspace().layers.satellites?.filters.groups).toEqual([
      'stations',
      'visual',
      'weather',
      'galileo',
      'science',
    ]);
    await user.click(screen.getByRole('switch', { name: 'GPS' }));
    expect(workspace().layers.satellites?.filters).toEqual({});
  });

  it('keeps list keys inside the options disclosure', async () => {
    const { user } = await setup();
    await user.click(screen.getByRole('button', { name: 'Aircraft options' }));
    const military = screen.getByRole('switch', { name: 'Military only' });
    military.focus();
    await user.keyboard('k');
    expect(military).toHaveFocus();
  });

  it('toggles a layer from the keyboard: j/k to move, Enter on the row', async () => {
    const { user, ...h } = await setup();
    const first = row('Earthquakes');
    first.focus();
    await user.keyboard('{Enter}');
    expect(h.runtime.stores.workspace.getState().layers.earthquakes?.enabled).toBe(true);
  });

  it('announces first data politely, at most once per 10 s per layer', async () => {
    const { user, ...h } = await setup();
    let now = Date.UTC(2026, 9, 11, 12);
    vi.spyOn(Date, 'now').mockImplementation(() => now);
    const status = screen.getByRole('status', { name: 'Layer updates' });
    const turnOnAndDeliver = async (ids: string[]) => {
      await user.click(screen.getByRole('switch', { name: 'Earthquakes' }));
      act(() => {
        h.platform.live('earthquakes')?.onEvent({ kind: 'snapshot', snapshot: quakeSnapshot(ids) });
      });
    };

    await turnOnAndDeliver(['a', 'b', 'c']);
    expect(status).toHaveTextContent('Earthquakes: 3 events');

    // Off and on again within 10 s: no second announcement.
    await user.click(screen.getByRole('switch', { name: 'Earthquakes' }));
    now += 4_000;
    await turnOnAndDeliver(['a', 'b', 'c', 'd', 'e']);
    expect(status).not.toHaveTextContent('5 events');

    // After the window, the next first-data is announced again.
    await user.click(screen.getByRole('switch', { name: 'Earthquakes' }));
    now += 11_000;
    await turnOnAndDeliver(['a']);
    expect(status).toHaveTextContent('Earthquakes: 1 event');
  });
});

describe('LayerRow rendering', () => {
  it('re-renders only the row whose feed updated', async () => {
    harness = await startHarness({ catalog: CATALOG });
    const h = harness;
    const quakes = CATALOG.layers[0];
    const planes = CATALOG.layers[1];
    if (!quakes || !planes) throw new Error('catalog fixture');
    act(() => {
      h.runtime.stores.workspace.getState().setLayerEnabled('earthquakes', true);
      h.runtime.stores.workspace.getState().setLayerEnabled('aircraft', true);
    });
    const commits: Record<string, number> = { earthquakes: 0, aircraft: 0 };
    const onRender: ProfilerOnRenderCallback = (id) => {
      commits[id] = (commits[id] ?? 0) + 1;
    };
    renderWithRuntime(
      h.runtime,
      <List aria-label="Layers">
        <Profiler id="earthquakes" onRender={onRender}>
          <LayerRow layer={quakes} sources="USGS" />
        </Profiler>
        <Profiler id="aircraft" onRender={onRender}>
          <LayerRow layer={planes} sources="adsb.lol" />
        </Profiler>
      </List>,
    );
    // Both rows have data, so both are subscribed to everything a row can show.
    act(() => {
      h.platform.live('aircraft')?.onEvent({
        kind: 'snapshot',
        snapshot: aircraftSnapshot([aircraft('4aca2d')]),
      });
      h.platform.live('earthquakes')?.onEvent({ kind: 'snapshot', snapshot: quakeSnapshot(['a']) });
    });

    const before = { ...commits };
    act(() => {
      h.platform.live('earthquakes')?.onEvent({
        kind: 'snapshot',
        snapshot: quakeSnapshot(['a', 'b']),
      });
    });
    expect(commits.earthquakes).toBeGreaterThan(before.earthquakes ?? 0);
    expect(commits.aircraft).toBe(before.aircraft);
    expect(screen.getByText(formatCount(2))).toBeInTheDocument();
  });
});
