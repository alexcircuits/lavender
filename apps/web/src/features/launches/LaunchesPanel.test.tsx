import { act, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { deliver, type Harness, renderWithRuntime, source, startHarness } from '../../app/testing';
import type { Catalog, Launch } from '../../contracts';
import { catalogOf, layer } from '../../test/fakes';
import { launch, launchSnapshot, padlessLaunch, WEST_PAD } from '../../test/launch';
import { LaunchesPanel } from './LaunchesPanel';

/** The fixture clock: after the first fixture launch, before the second. */
const NOW = Date.parse('2026-06-02T12:00:00Z');

const CATALOG: Catalog = catalogOf(
  [
    layer('launches', {
      name: 'Rocket launches',
      family: 'space',
      sourceIds: ['launch-library-2'],
    }),
  ],
  {
    sources: [
      source('launch-library-2', {
        name: 'The Space Devs — Launch Library 2',
        attribution: 'Launch Library 2 — The Space Devs',
      }),
    ],
  },
);

const FIXTURE_LIKE = [
  launch('sirius', {
    mission: 'Sirius SXM-11',
    status: 'Success',
    net: '2026-06-01T12:00:00Z',
  }),
  launch('starlink', {
    mission: 'Starlink Group 17-40',
    status: 'Go for Launch',
    net: '2026-06-02T18:00:00Z',
    pad: WEST_PAD,
  }),
  launch('cygnus', { mission: 'Cygnus NG-21', status: 'Success', net: '2026-08-04T15:02:00Z' }),
  launch('late', { mission: 'Late Flight', status: 'Go for Launch', net: '2026-05-30T08:00:00Z' }),
];

let harness: Harness | null = null;

async function setup(catalog: Catalog = CATALOG, enabled = true) {
  harness = await startHarness({ catalog, globe: true });
  const h = harness;
  if (enabled) act(() => h.runtime.stores.workspace.getState().setLayerEnabled('launches', true));
  renderWithRuntime(h.runtime, <LaunchesPanel catalog={catalog} />);
  return { ...h, user: userEvent.setup() };
}

const optionNames = (list: string) =>
  within(screen.getByRole('listbox', { name: list }))
    .getAllByRole('option')
    .map((option) => option.querySelector('[class*="titleText"]')?.textContent);

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['Date'], now: NOW });
});

afterEach(() => {
  harness?.stop();
  harness = null;
  vi.useRealTimers();
});

describe('LaunchesPanel', () => {
  it('offers to turn the layer on while it is off, then says it is connecting', async () => {
    const { runtime, user, platform } = await setup(CATALOG, false);
    expect(screen.getByText(/The Rocket launches layer is off/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Turn on Rocket launches' }));
    expect(runtime.stores.workspace.getState().layers.launches?.enabled).toBe(true);
    expect(platform.live('launches')?.query).toEqual({ feed: 'launches', filters: {} });
    expect(screen.getByText('Connecting…')).toBeInTheDocument();
  });

  it('lists upcoming soonest first and recent latest first, on the fixture clock', async () => {
    const h = await setup();
    deliver(h, launchSnapshot(FIXTURE_LIKE, { notices: ['Recorded fixture data, not live.'] }));
    expect(optionNames('Upcoming launches')).toEqual(['Starlink Group 17-40', 'Cygnus NG-21']);
    expect(optionNames('Recent launches')).toEqual(['Sirius SXM-11', 'Late Flight']);

    const starlink = screen.getByRole('option', { name: /Starlink Group 17-40/ });
    expect(within(starlink).getByText('in 6 h')).toBeInTheDocument();
    expect(within(starlink).getByText('2026-06-02 18:00 UTC')).toHaveClass('mono');
    expect(within(starlink).getByText('Go')).toBeInTheDocument();
    expect(within(starlink).getByText('Test Provider · Test Rocket Block 1')).toBeInTheDocument();
    expect(within(starlink).getByText('Test Complex 4E')).toBeInTheDocument();
    const sirius = screen.getByRole('option', { name: /Sirius SXM-11/ });
    expect(within(sirius).getByText('24 h ago')).toBeInTheDocument();
    expect(within(sirius).getByText('Success')).toBeInTheDocument();
    // A passed NET still reported as "Go" says the outcome is missing instead of hiding it.
    const late = screen.getByRole('option', { name: /Late Flight/ });
    expect(within(late).getByText('No outcome reported yet')).toBeInTheDocument();
    expect(within(sirius).queryByText('No outcome reported yet')).not.toBeInTheDocument();

    expect(screen.getByText('4 launches')).toBeInTheDocument();
    expect(screen.getByText('Recorded fixture data, not live.')).toBeInTheDocument();
    expect(screen.getByText('Source: The Space Devs — Launch Library 2')).toBeInTheDocument();
    // Live data carries no "Live" pill here, as in the layer row.
    expect(screen.queryByText('Live')).not.toBeInTheDocument();
  });

  it('selects a launch, flies to its pad and marks its row; the keyboard does the same', async () => {
    const h = await setup();
    const nopad = padlessLaunch('nopad', { mission: 'No Pad', net: '2026-07-01T00:00:00Z' });
    deliver(h, launchSnapshot([...FIXTURE_LIKE, nopad]));
    await h.user.click(screen.getByRole('option', { name: /Starlink Group 17-40/ }));
    expect(h.runtime.stores.workspace.getState().selection).toEqual({
      feed: 'launches',
      id: 'starlink',
    });
    expect(h.globe?.globe.flyTo).toHaveBeenLastCalledWith({
      kind: 'point',
      position: { lat: WEST_PAD.latitude, lon: WEST_PAD.longitude },
    });
    expect(screen.getByRole('option', { name: /Starlink Group 17-40/ })).toHaveAttribute(
      'aria-selected',
      'true',
    );

    // j/arrows move between rows, Enter selects; a launch without a pad selects without flying.
    screen.getByRole('option', { name: /Starlink Group 17-40/ }).focus();
    await h.user.keyboard('j');
    expect(screen.getByRole('option', { name: /No Pad/ })).toHaveFocus();
    await h.user.keyboard('j');
    expect(screen.getByRole('option', { name: /Cygnus NG-21/ })).toHaveFocus();
    await h.user.keyboard('k');
    expect(screen.getByRole('option', { name: /No Pad/ })).toHaveFocus();
    expect(screen.getByRole('option', { name: /No Pad/ })).toHaveTextContent('Pad not reported');
    const flights = vi.mocked(h.globe?.globe.flyTo)?.mock.calls.length;
    await h.user.keyboard('{Enter}');
    expect(h.runtime.stores.workspace.getState().selection).toEqual({
      feed: 'launches',
      id: 'nopad',
    });
    expect(vi.mocked(h.globe?.globe.flyTo)?.mock.calls.length).toBe(flights);
  });

  it('says when the feed failed before any data, and keeps the list with its pill after', async () => {
    const h = await setup();
    act(() =>
      h.platform.live('launches')?.onEvent({
        kind: 'error',
        error: { code: 'upstreamUnavailable', message: 'LL2 did not answer.', retryable: true },
      }),
    );
    expect(screen.getByText('The Rocket launches layer is unavailable')).toBeInTheDocument();
    expect(screen.getByText(/LL2 did not answer\./)).toBeInTheDocument();

    deliver(h, launchSnapshot(FIXTURE_LIKE, { freshness: { state: 'stale', ageS: 5_400 } }));
    expect(screen.getByText('Stale')).toBeInTheDocument();
    expect(screen.getByText('1 h 30 min')).toBeInTheDocument();
    expect(screen.getAllByRole('option')).toHaveLength(4);
  });

  it('says once why there is nothing to list when the first fetch failed upstream', async () => {
    // The server's answer to a failed first fetch: a snapshot with no items, marked unavailable.
    const h = await setup();
    deliver(
      h,
      launchSnapshot([], {
        freshness: { state: 'unavailable', reason: 'Launch Library 2 answered HTTP 429' },
      }),
    );
    expect(screen.getByText('The Rocket launches layer is unavailable')).toBeInTheDocument();
    expect(screen.getAllByText(/HTTP 429/)).toHaveLength(1);
    expect(
      screen.getByText('Launch Library 2 answered HTTP 429. Lavender keeps trying.'),
    ).toBeInTheDocument();
    expect(screen.getByText('Unavailable')).toBeInTheDocument();
    // No data is not a count of launches.
    expect(screen.queryByText(/^0 launches$/)).not.toBeInTheDocument();
  });

  it('skips a malformed record instead of failing the whole list', async () => {
    const h = await setup();
    const broken = { ...launch('broken'), status: undefined, name: 7 } as unknown as Launch;
    deliver(h, launchSnapshot([...FIXTURE_LIKE, broken]));
    expect(screen.getAllByRole('option')).toHaveLength(FIXTURE_LIKE.length);
    // The count is of the launches listed.
    expect(screen.getByText(`${FIXTURE_LIKE.length} launches`)).toBeInTheDocument();
  });

  it('says when the source reports nothing, per group and overall', async () => {
    const h = await setup();
    deliver(h, launchSnapshot([launch('old', { net: '2026-05-01T00:00:00Z' })]));
    expect(screen.getByText('No upcoming launches reported.')).toBeInTheDocument();
    deliver(h, launchSnapshot([]));
    expect(
      screen.getByText('The source reports no recent or upcoming launches.'),
    ).toBeInTheDocument();
  });

  it('says when the server offers no launches layer', async () => {
    await setup(catalogOf([layer('earthquakes')]), false);
    expect(screen.getByText('This server does not offer rocket launches.')).toBeInTheDocument();
  });
});
