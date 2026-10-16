import { act, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  deliver,
  type Harness,
  renderWithRuntime,
  satellite,
  satelliteSnapshot,
  source,
  startHarness,
} from '../../app/testing';
import type { Catalog } from '../../contracts';
import { padIdOf } from '../../lib/launches';
import { catalogOf, layer } from '../../test/fakes';
import { CAPE_PAD, launch, launchSnapshot, WEST_PAD } from '../../test/launch';
import { Inspector } from './Inspector';

const NOW = Date.parse('2026-06-02T12:00:00Z');
const CAPE = padIdOf(CAPE_PAD);
const NOTICE = 'Recorded fixture data, not live.';

const CATALOG: Catalog = catalogOf(
  [
    layer('launches', {
      name: 'Rocket launches',
      family: 'space',
      sourceIds: ['launch-library-2'],
    }),
    layer('satellites', { name: 'Satellites', sourceIds: ['celestrak'] }),
  ],
  {
    sources: [
      source('launch-library-2', {
        name: 'The Space Devs — Launch Library 2',
        attribution: 'Launch Library 2 — The Space Devs',
      }),
      source('celestrak', { name: 'CelesTrak' }),
    ],
  },
);

const SXM = launch('sxm', {
  name: 'Test Rocket | Sirius SXM-11',
  mission: 'Sirius SXM-11',
  missionDescription: 'High-powered broadcasting satellite.',
  status: 'Success',
  net: '2026-06-01T12:00:00Z',
  windowStart: '2026-06-01T11:30:00Z',
  windowEnd: '2026-06-01T13:30:00Z',
  payloadNames: ['SXM-11'],
  matchedNoradIds: [25544, 99999],
});
const CYGNUS = launch('cygnus', { mission: 'Cygnus NG-21', net: '2026-08-04T15:02:00Z' });
const STARLINK = launch('starlink', { mission: 'Starlink Group 17-40', pad: WEST_PAD });

let harness: Harness | null = null;

async function setup() {
  harness = await startHarness({ catalog: CATALOG, globe: true });
  const h = harness;
  act(() => h.runtime.stores.workspace.getState().setLayerEnabled('launches', true));
  renderWithRuntime(h.runtime, <Inspector />);
  deliver(h, launchSnapshot([SXM, CYGNUS, STARLINK], { notices: [NOTICE] }));
  return { ...h, user: userEvent.setup() };
}

function select(h: Harness, feed: 'launches' | 'satellites', id: string) {
  act(() => h.runtime.stores.workspace.getState().select({ feed, id }));
}

function fact(sheet: string, label: string): string {
  const term = within(screen.getByLabelText(sheet)).getByText(label, { selector: 'dt' });
  return term.nextElementSibling?.textContent ?? '';
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['Date'], now: NOW });
});

afterEach(() => {
  harness?.stop();
  harness = null;
  vi.useRealTimers();
});

describe('launch inspector sheet', () => {
  it('shows the launch, its window, mission, payloads, attribution and fixture provenance', async () => {
    const h = await setup();
    select(h, 'launches', 'sxm');
    const region = screen.getByRole('region', { name: 'Sirius SXM-11' });
    expect(within(region).getByText('Test Provider · Test Rocket Block 1')).toBeInTheDocument();
    expect(within(region).getByText(NOTICE)).toBeInTheDocument();
    expect(fact('Facts', 'Status')).toBe('Success');
    expect(fact('Facts', 'No earlier than (NET)')).toBe('2026-06-01 12:00:00 UTC · 24 h ago');
    expect(fact('Facts', 'Window')).toBe('2026-06-01 11:30–13:30 UTC (2 h)');
    expect(fact('Facts', 'Pad')).toBe('Test Complex 40');
    expect(within(region).getByText('High-powered broadcasting satellite.')).toBeInTheDocument();
    expect(
      within(screen.getByRole('list', { name: 'Payloads' })).getByText('SXM-11'),
    ).toBeVisible();
    expect(within(region).getByText('Launch Library 2 — The Space Devs')).toHaveAttribute(
      'data-attribution',
    );
    expect(fact('Provenance', 'Source')).toBe('The Space Devs — Launch Library 2');
    expect(within(region).getByRole('button', { name: 'Fly to' })).toBeEnabled();
  });

  it('reads the launch clock the list reads, so a NET passed seconds ago is still ahead', async () => {
    // 45 s into the minute, a NET 30 s into it: the list (to the minute) still has it upcoming.
    vi.setSystemTime(Date.parse('2026-06-02T12:00:45Z'));
    const h = await setup();
    deliver(h, launchSnapshot([launch('soon', { net: '2026-06-02T12:00:30Z' })]));
    select(h, 'launches', 'soon');
    expect(fact('Facts', 'Status')).toBe('Go');
    expect(fact('Facts', 'No earlier than (NET)')).toBe('2026-06-02 12:00:30 UTC · now');
  });

  it('lists the other launches from the same pad as links that select them', async () => {
    const h = await setup();
    select(h, 'launches', 'sxm');
    const others = screen.getByRole('list', { name: 'Other launches from Test Complex 40' });
    await h.user.click(within(others).getByRole('button', { name: 'Cygnus NG-21' }));
    expect(h.runtime.stores.workspace.getState().selection).toEqual({
      feed: 'launches',
      id: 'cygnus',
    });
    select(h, 'launches', 'starlink');
    expect(screen.queryByText(/^Also from/)).not.toBeInTheDocument();
  });
});

describe('matched satellites', () => {
  it('offers to turn satellites on, then links the ones the layer has', async () => {
    const h = await setup();
    select(h, 'launches', 'sxm');
    const matched = () => screen.getByRole('list', { name: 'Matched satellites' });
    expect(within(matched()).getByText('NORAD 25544')).toBeInTheDocument();
    expect(within(matched()).queryByRole('button')).not.toBeInTheDocument();
    expect(screen.getByText('Turn on Satellites to select them.')).toBeInTheDocument();

    await h.user.click(screen.getByRole('button', { name: 'Turn on Satellites' }));
    expect(h.runtime.stores.workspace.getState().layers.satellites?.enabled).toBe(true);
    expect(screen.getByText('Waiting for Satellites data…')).toBeInTheDocument();

    deliver(h, satelliteSnapshot([satellite(25544)]));
    const link = within(matched()).getByRole('button', { name: 'ISS (ZARYA) · NORAD 25544' });
    expect(within(matched()).getByText('Not in the groups shown')).toBeInTheDocument();
    expect(screen.getByText(/^Some are not in the Satellites groups shown/)).toBeInTheDocument();
    await h.user.click(link);
    expect(h.runtime.stores.workspace.getState().selection).toEqual({
      feed: 'satellites',
      id: '25544',
    });
  });

  it('says plainly when nothing was matched', async () => {
    const h = await setup();
    select(h, 'launches', 'cygnus');
    expect(
      screen.getByText(/^No tracked satellites are matched to this launch\./),
    ).toBeInTheDocument();
    expect(screen.queryByRole('list', { name: 'Matched satellites' })).not.toBeInTheDocument();
  });
});

describe('launch pad inspector sheet', () => {
  it('lists the pad’s launches, upcoming then recent, as links to their sheets', async () => {
    const h = await setup();
    select(h, 'launches', CAPE);
    const region = screen.getByRole('region', { name: 'Test Complex 40' });
    expect(within(region).getByText('Launch pad · Test Cape, FL, USA')).toBeInTheDocument();
    expect(within(region).getByText(NOTICE)).toBeInTheDocument();
    expect(fact('Facts', 'Launches')).toBe('2 · 1 upcoming, 1 recent');
    expect(fact('Facts', 'Next launch')).toBe('Cygnus NG-21 · in 63 d');
    const upcoming = screen.getByRole('list', { name: 'Upcoming launches from this pad' });
    expect(within(upcoming).getByRole('button', { name: 'Cygnus NG-21' })).toBeInTheDocument();
    const recent = screen.getByRole('list', { name: 'Recent launches from this pad' });
    expect(within(recent).getByText('Success')).toBeInTheDocument();
    expect(within(region).queryByText('No longer in feed')).not.toBeInTheDocument();
    await h.user.click(within(recent).getByRole('button', { name: 'Sirius SXM-11' }));
    expect(h.runtime.stores.workspace.getState().selection).toEqual({
      feed: 'launches',
      id: 'sxm',
    });
  });

  it('keeps a pad that left the feed with its last launches, and says so', async () => {
    const h = await setup();
    select(h, 'launches', CAPE);
    deliver(h, launchSnapshot([STARLINK]));
    const region = screen.getByRole('region', { name: 'Test Complex 40' });
    expect(within(region).getByRole('status')).toHaveTextContent('No longer in feed');
    expect(fact('Facts', 'Launches')).toBe('2 · 1 upcoming, 1 recent');
  });

  it('explains a pad selection while the launches layer is off', async () => {
    const h = await setup();
    act(() => h.runtime.stores.workspace.getState().setLayerEnabled('launches', false));
    select(h, 'launches', CAPE);
    const region = screen.getByRole('region', { name: 'Launch pad' });
    expect(
      within(region).getByText(
        'This launch pad is in Rocket launches, which is off. Turn it on to see it.',
      ),
    ).toBeInTheDocument();
  });
});
