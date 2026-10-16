import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  aircraft,
  aircraftSnapshot,
  type Harness,
  renderWithRuntime,
  satellite,
  satelliteSnapshot,
  source,
  startHarness,
} from '../../app/testing';
import type {
  Earthquake,
  FeedId,
  FeedSnapshot,
  FireCell,
  SatelliteTrack,
  VesselTrackResponse,
} from '../../contracts';
import { clearToasts, formatCount } from '../../design';
import { PlatformError } from '../../platform';
import { catalogOf, type FakePlatform, layer, quakeSnapshot } from '../../test/fakes';
import { vessel, vesselSnapshot } from '../../test/vessel';
import { useCommandContext, useGlobalShortcuts } from '../commands';
import { Inspector } from './Inspector';

const CATALOG = catalogOf(
  [
    layer('aircraft', { name: 'Aircraft', sourceIds: ['adsb-lol'] }),
    layer('earthquakes', { name: 'Earthquakes', sourceIds: ['usgs-earthquakes'] }),
    layer('satellites', { name: 'Satellites', sourceIds: ['celestrak'] }),
  ],
  {
    sources: [
      source('adsb-lol', { name: 'adsb.lol' }),
      source('usgs-earthquakes', { name: 'USGS' }),
      source('celestrak', { name: 'CelesTrak' }),
    ],
  },
);

function WithShortcuts() {
  useGlobalShortcuts(useCommandContext());
  return <Inspector />;
}

let harness: Harness | null = null;

async function setup(feed: FeedId) {
  harness = await startHarness({ catalog: CATALOG, globe: true });
  const h = harness;
  act(() => h.runtime.stores.workspace.getState().setLayerEnabled(feed, true));
  renderWithRuntime(h.runtime, <WithShortcuts />);
  return { ...h, user: userEvent.setup() };
}

/** The inspector island, if open (the toaster is a region too). */
const inspector = () => document.querySelector('[data-inspector]');

function select(h: Harness, feed: FeedId, id: string) {
  act(() => h.runtime.stores.workspace.getState().select({ feed, id }));
}

function fact(sheet: string, label: string): string {
  const sheetEl = screen.getByLabelText(sheet);
  const term = within(sheetEl).getByText(label, { selector: 'dt' });
  return term.nextElementSibling?.textContent ?? '';
}

afterEach(() => {
  harness?.stop();
  harness = null;
  clearToasts();
  vi.restoreAllMocks();
});

describe('Inspector', () => {
  it('shows a loaded vessel without inventing observation times or tracking actions', async () => {
    harness = await startHarness({
      catalog: catalogOf([layer('vessels', { family: 'maritime' })]),
      globe: true,
    });
    const h = harness;
    act(() => h.runtime.stores.workspace.getState().setLayerEnabled('vessels', true));
    renderWithRuntime(h.runtime, <Inspector />);
    h.platform.live('vessels')?.onEvent({
      kind: 'snapshot',
      snapshot: vesselSnapshot([vessel({ name: 'TEST SHIP', speedKnots: 12.4 })]),
    });
    select(h, 'vessels', '123456789');
    const region = screen.getByRole('region', { name: 'TEST SHIP' });
    expect(fact('Facts', 'MMSI')).toBe('123456789');
    expect(fact('Facts', 'Speed over ground')).toBe('12.4 kn');
    expect(fact('Facts', 'Heading')).toBe('Not reported');
    expect(fact('Facts', 'Position time')).toBe('Not reported');
    expect(fact('Facts', 'Position age')).toBe('Not reported');
    expect(fact('Provenance', 'Observed')).toBe('Not reported');
    // The recent track is the platform's retained data; no follow action is offered.
    expect(within(region).queryByRole('button', { name: /Follow/ })).not.toBeInTheDocument();
    await waitFor(() =>
      expect(h.platform.vesselTrack).toHaveBeenCalledWith('123456789', expect.anything()),
    );
  });

  it('loads the selected vessel track, draws it on the globe and clears it with the sheet', async () => {
    harness = await startHarness({
      catalog: catalogOf([layer('vessels', { family: 'maritime', sourceIds: ['aisstream'] })], {
        sources: [source('aisstream', { name: 'AISStream' })],
      }),
      globe: true,
    });
    const h = harness;
    const track: VesselTrackResponse = {
      mmsi: '230990001',
      source: 'aisstream',
      retainedSec: 1_800,
      samples: [
        { lat: 60.1, lon: 24.9, epochSec: Date.parse('2026-10-15T11:50:00Z') / 1000 },
        { lat: 60.1, lon: 24.91, epochSec: Date.parse('2026-10-15T11:51:00Z') / 1000 },
        { lat: 60.1, lon: 24.95, epochSec: Date.parse('2026-10-15T11:55:00Z') / 1000 },
      ],
    };
    h.platform.vesselTrack.mockResolvedValue(track);
    act(() => h.runtime.stores.workspace.getState().setLayerEnabled('vessels', true));
    renderWithRuntime(h.runtime, <Inspector />);
    h.platform.live('vessels')?.onEvent({
      kind: 'snapshot',
      snapshot: {
        ...vesselSnapshot([
          vessel({
            mmsi: '230990001',
            name: 'TEST TRADER',
            lastPositionUtc: '2026-10-15T11:55:00Z',
          }),
        ]),
        sources: ['aisstream'],
        notices: ['Recorded fixture data, not live.', 'Synthetic AIS replay.'],
      },
    });
    select(h, 'vessels', '230990001');
    const status = await screen.findByText('3 positions over 5 min · AISStream');
    expect(status).toHaveAttribute('role', 'status');
    const rows = within(screen.getByRole('table', { name: 'Vessel track' })).getAllByRole('row');
    // Header, then newest first.
    expect(rows[1]).toHaveTextContent('11:55:00 UTC');
    expect(h.globe?.globe.showVesselTrack).toHaveBeenLastCalledWith(track);
    expect(fact('Provenance', 'Source')).toBe('AISStream');
    expect(fact('Provenance', 'Notices')).toBe(
      'Recorded fixture data, not live. Synthetic AIS replay.',
    );

    await userEvent.setup().click(screen.getByRole('button', { name: 'Refresh track' }));
    await waitFor(() => expect(h.platform.vesselTrack).toHaveBeenCalledTimes(2));

    select(h, 'vessels', '230990002');
    expect(h.globe?.globe.showVesselTrack).toHaveBeenLastCalledWith(null);
  });

  it('says when the server has kept no positions for the vessel', async () => {
    harness = await startHarness({
      catalog: catalogOf([layer('vessels', { family: 'maritime' })]),
      globe: true,
    });
    const h = harness;
    h.platform.vesselTrack.mockRejectedValue(
      new PlatformError({
        code: 'notFound',
        message: 'No retained track is available for vessel 123456789.',
        retryable: false,
      }),
    );
    act(() => h.runtime.stores.workspace.getState().setLayerEnabled('vessels', true));
    renderWithRuntime(h.runtime, <Inspector />);
    h.platform.live('vessels')?.onEvent({
      kind: 'snapshot',
      snapshot: vesselSnapshot([vessel()]),
    });
    select(h, 'vessels', '123456789');
    expect(
      await screen.findByText('This server has kept no positions for this vessel yet.'),
    ).toBeInTheDocument();
    expect(screen.queryByRole('table', { name: 'Vessel track' })).not.toBeInTheDocument();
    expect(h.globe?.globe.showVesselTrack).not.toHaveBeenCalledWith(
      expect.objectContaining({ mmsi: '123456789' }),
    );
  });

  it('describes fire cells with malformed fields and never crashes on undrawable ones', async () => {
    harness = await startHarness({
      catalog: catalogOf([layer('fires', { name: 'Active fires' })]),
      globe: true,
    });
    const h = harness;
    act(() => h.runtime.stores.workspace.getState().setLayerEnabled('fires', true));
    renderWithRuntime(h.runtime, <Inspector />);
    const fire: FireCell = {
      lat: 34.05,
      lon: -118.25,
      brightness: 0,
      frp: 18.2,
      confidence: 'nominal',
      satellite: 'N20',
      acquiredAt: '2026-10-15T12:00:00Z',
      dayNight: 'D',
      count: 1,
      maxFrp: 18.2,
      latestAcquiredAt: '2026-10-15T12:00:00Z',
    };
    // Wire data a broken or newer server could send (JSON carries NaN as null).
    const items = [
      { ...fire, satellite: null, confidence: 3 },
      { ...fire, lat: 10, count: 0 },
      { ...fire, lat: 11, frp: null },
      { ...fire, lat: 12, maxFrp: -2 },
    ] as unknown as FireCell[];
    const snapshot: FeedSnapshot = {
      feed: 'fires',
      sources: ['fires-source'],
      fetchedAt: '2026-10-15T12:00:00Z',
      generatedAt: '2026-10-15T12:00:01Z',
      freshness: { state: 'live' },
      coverage: { kind: 'complete' },
      sourceCount: 1,
      pendingRegions: 0,
      items: { kind: 'fires', items },
      notices: [],
    };
    h.platform.live('fires')?.onEvent({ kind: 'snapshot', snapshot });

    select(h, 'fires', '34.0500,-118.2500');
    expect(screen.getByRole('region', { name: 'Active fire detection' })).toBeInTheDocument();
    expect(fact('Facts', 'Radiative power')).toBe('18.2 MW');
    expect(fact('Facts', 'Brightness')).toBe('Not reported');
    expect(fact('Facts', 'Satellite')).toBe('Not reported');
    expect(fact('Facts', 'Confidence')).toBe('Not reported');
    expect(screen.getByText('FRP 18.2 MW · Satellite')).toBeInTheDocument();

    for (const id of ['10.0000,-118.2500', '11.0000,-118.2500', '12.0000,-118.2500']) {
      select(h, 'fires', id);
      expect(
        screen.getByText(
          `${id} is not in the latest Active fires data for the area the globe shows.`,
        ),
      ).toBeVisible();
    }
  });

  it('loads observed aircraft history on demand with source and timestamps', async () => {
    const h = await setup('aircraft');
    h.platform
      .live('aircraft')
      ?.onEvent({ kind: 'snapshot', snapshot: aircraftSnapshot([aircraft('4aca2d')]) });
    h.platform.aircraftTrack.mockResolvedValue({
      hex: '4aca2d',
      sources: ['adsb-lol'],
      points: [{ time: '2026-10-11T11:59:57Z', position: { lat: 60.1699, lon: 24.9384 } }],
    });
    select(h, 'aircraft', '4aca2d');
    expect(h.platform.aircraftTrack).not.toHaveBeenCalled();
    await h.user.click(screen.getByRole('button', { name: 'Load recent history' }));
    const table = await screen.findByRole('table', { name: 'Aircraft history' });
    expect(within(table).getByText('11:59:57 UTC')).toBeVisible();
    expect(within(table).getByText('60.1699° N, 24.9384° E')).toBeVisible();
    expect(screen.getByText('1 retained observations · adsb.lol')).toBeVisible();
    expect(h.platform.aircraftTrack).toHaveBeenCalledWith(
      '4aca2d',
      expect.objectContaining({ signal: expect.any(AbortSignal) }),
    );
    expect(screen.getByRole('button', { name: 'Refresh history' })).toBeVisible();
  });

  it('shows a history error and cancels pending work when selection changes', async () => {
    const h = await setup('aircraft');
    h.platform.live('aircraft')?.onEvent({
      kind: 'snapshot',
      snapshot: aircraftSnapshot([aircraft('4aca2d'), aircraft('abcdef')]),
    });
    h.platform.aircraftTrack.mockRejectedValueOnce({
      code: 'notFound',
      message: 'No retained track is available.',
      retryable: false,
    });
    select(h, 'aircraft', '4aca2d');
    await h.user.click(screen.getByRole('button', { name: 'Load recent history' }));
    expect(await screen.findByText('No retained track is available.')).toBeVisible();
    h.platform.aircraftTrack.mockImplementationOnce(() => new Promise(() => {}));
    await h.user.click(screen.getByRole('button', { name: 'Load recent history' }));
    const options = h.platform.aircraftTrack.mock.calls.at(-1)?.[1];
    select(h, 'aircraft', 'abcdef');
    expect(options?.signal?.aborted).toBe(true);
    expect(screen.queryByText('No retained track is available.')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Load recent history' })).toBeVisible();
  });

  it('renders nothing without a selection', async () => {
    harness = await startHarness({ catalog: CATALOG });
    renderWithRuntime(harness.runtime, <Inspector />);
    expect(inspector()).toBeNull();
  });

  it('shows an aircraft: identity, kinematics, position and provenance', async () => {
    const h = await setup('aircraft');
    h.platform.live('aircraft')?.onEvent({
      kind: 'snapshot',
      snapshot: aircraftSnapshot([
        aircraft('4aca2d', {
          callsign: 'SAS1462',
          registration: 'SE-ROM',
          typeCode: 'A20N',
          altitudeBaroFt: 36_000,
          groundSpeedKt: 451,
          trackDeg: 214.4,
          verticalRateFpm: -64,
          squawk: '7700',
          emergency: 'general',
          seenPosS: 3,
        }),
      ]),
    });
    select(h, 'aircraft', '4aca2d');

    const region = screen.getByRole('region', { name: 'SAS1462' });
    expect(within(region).getByText('A20N · SE-ROM')).toBeInTheDocument();
    expect(fact('Facts', 'ICAO address')).toBe('4aca2d');
    expect(fact('Facts', 'Altitude')).toBe(`${formatCount(36_000)} ft`);
    expect(fact('Facts', 'Ground speed')).toBe('451 kn');
    expect(fact('Facts', 'Track')).toBe('214°');
    expect(fact('Facts', 'Vertical rate')).toBe('−64 ft/min');
    expect(fact('Facts', 'Squawk')).toBe('7700');
    expect(fact('Facts', 'Emergency')).toBe('Declared: general');
    expect(fact('Facts', 'Military')).toBe('No');
    expect(fact('Facts', 'Position')).toBe('60.1699° N, 24.9384° E');
    expect(fact('Facts', 'MGRS')).toMatch(/^35VLG \d{5} \d{5}$/);
    expect(fact('Facts', 'Position age')).toBe('3 s');
    expect(fact('Provenance', 'Source')).toBe('adsb.lol');
    expect(fact('Provenance', 'Observed')).toBe('11:59:57 UTC');
    expect(fact('Provenance', 'Fetched')).toBe('12:00:00 UTC');
    // v1 actions only: no dead Track/Follow buttons.
    expect(within(region).queryByRole('button', { name: /Track|Follow/ })).not.toBeInTheDocument();
  });

  it('flies to the entity and copies its coordinates without moving the camera itself', async () => {
    const h = await setup('aircraft');
    h.platform.live('aircraft')?.onEvent({
      kind: 'snapshot',
      snapshot: aircraftSnapshot([aircraft('4aca2d')]),
    });
    select(h, 'aircraft', '4aca2d');
    expect(h.globe?.globe.flyTo).not.toHaveBeenCalled();

    await h.user.click(screen.getByRole('button', { name: 'Fly to' }));
    expect(h.globe?.globe.flyTo).toHaveBeenCalledWith({
      kind: 'point',
      position: { lat: 60.1699, lon: 24.9384 },
    });

    const writeText = vi.fn(async () => undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    await h.user.click(screen.getByRole('button', { name: 'Copy coordinates' }));
    expect(writeText).toHaveBeenCalledWith('60.1699° N, 24.9384° E');
    expect(await screen.findByText('Coordinates copied')).toBeInTheDocument();

    writeText.mockRejectedValueOnce(new Error('denied'));
    await h.user.click(screen.getByRole('button', { name: 'Copy coordinates' }));
    expect(await screen.findByText('Coordinates not copied')).toBeInTheDocument();
  });

  it('shows an earthquake and links its USGS event page only', async () => {
    const h = await setup('earthquakes');
    const quake: Earthquake = {
      id: 'us7000abcd',
      magnitude: 4.62,
      magnitudeType: 'mb',
      depthKm: 10,
      position: { lat: 38.897676, lon: -77.036548 },
      place: '12 km SSW of Ridgecrest, CA',
      time: '2026-10-11T11:00:00Z',
      updated: '2026-10-11T11:20:00Z',
      url: 'https://earthquake.usgs.gov/earthquakes/eventpage/us7000abcd',
      tsunami: true,
      alert: 'orange',
      felt: 12,
      significance: 512,
      status: 'reviewed',
    };
    h.platform.live('earthquakes')?.onEvent({
      kind: 'snapshot',
      snapshot: { ...quakeSnapshot([]), items: { kind: 'earthquakes', items: [quake] } },
    });
    select(h, 'earthquakes', 'us7000abcd');

    const region = screen.getByRole('region', { name: 'M 4.6 earthquake' });
    // The summary line under the title names the place; the facts repeat it.
    expect(within(region).getAllByText('12 km SSW of Ridgecrest, CA')).toHaveLength(2);
    expect(fact('Facts', 'Magnitude')).toBe('4.6 mb');
    expect(fact('Facts', 'Depth')).toBe('10 km');
    expect(fact('Facts', 'Time')).toMatch(/^2026-10-11 11:00:00 UTC · .+ ago$/);
    expect(fact('Facts', 'Tsunami')).toBe('Flagged: check official warnings');
    expect(fact('Facts', 'PAGER alert')).toBe('Orange');
    expect(fact('Facts', 'Felt')).toBe('12 reports');
    expect(fact('Facts', 'Significance')).toBe('512');
    expect(fact('Facts', 'Review status')).toBe('Reviewed');
    expect(fact('Facts', 'MGRS')).toBe('18SUJ 23390 07393');
    expect(fact('Provenance', 'Source')).toBe('USGS');
    expect(fact('Provenance', 'Observed')).toBe('11:00:00 UTC');

    const link = within(region).getByRole('link', { name: 'Open source record' });
    expect(link).toHaveAttribute('href', quake.url);
    expect(link).toHaveAttribute('target', '_blank');
    expect(link).toHaveAttribute('rel', 'noopener noreferrer');

    act(() => {
      h.platform.live('earthquakes')?.onEvent({
        kind: 'snapshot',
        snapshot: {
          ...quakeSnapshot([]),
          items: {
            kind: 'earthquakes',
            items: [{ ...quake, url: 'https://usgs.gov.evil.example/x' }],
          },
        },
      });
    });
    expect(within(region).queryByRole('link')).not.toBeInTheDocument();
  });

  it('shows a satellite with orbit figures and its current sub-satellite point', async () => {
    const h = await setup('satellites');
    const track: SatelliteTrack = {
      noradId: 25544,
      name: 'ISS',
      periodMin: 92.9,
      points: [{ time: '2026-10-11T12:00:00Z', position: { lat: 51.2, lon: -0.1, altM: 418_000 } }],
    };
    // The fake's return type is inferred from its empty default track, so widen it here.
    h.platform.satelliteTrack.mockResolvedValue(
      track as Awaited<ReturnType<FakePlatform['satelliteTrack']>>,
    );
    h.platform.live('satellites')?.onEvent({
      kind: 'snapshot',
      snapshot: satelliteSnapshot([satellite(25544)]),
    });
    select(h, 'satellites', '25544');

    const region = screen.getByRole('region', { name: 'ISS (ZARYA)' });
    expect(within(region).getByText('NORAD 25544 · Space stations')).toBeInTheDocument();
    expect(fact('Facts', 'NORAD ID')).toBe('25544');
    expect(fact('Facts', 'International designator')).toBe('1998-067A');
    expect(fact('Facts', 'Inclination')).toBe('51.64°');
    expect(fact('Facts', 'Period')).toBe('92.9 min');
    expect(fact('Facts', 'Perigee (approx.)')).toMatch(/^4\d\d km$/);
    expect(fact('Facts', 'Epoch')).toBe('2026-10-11 06:00:00 UTC');
    expect(await within(region).findByText('51.2000° N, 0.1000° W')).toBeInTheDocument();
    expect(fact('Facts', 'Altitude')).toBe('418 km');
    expect(fact('Facts', 'Position time')).toBe('12:00:00 UTC');
    expect(fact('Provenance', 'Source')).toBe('CelesTrak');

    await h.user.click(screen.getByRole('button', { name: 'Fly to' }));
    expect(h.globe?.globe.flyTo).toHaveBeenCalledWith({
      kind: 'point',
      position: { lat: 51.2, lon: -0.1, altM: 418_000 },
    });
  });

  it('says when the entity has left its feed, and when it was last seen', async () => {
    const h = await setup('aircraft');
    h.platform.live('aircraft')?.onEvent({
      kind: 'snapshot',
      snapshot: aircraftSnapshot([aircraft('4aca2d', { callsign: 'SAS1462' })]),
    });
    select(h, 'aircraft', '4aca2d');
    expect(screen.queryByText(/No longer in feed/)).not.toBeInTheDocument();

    act(() => {
      h.platform.live('aircraft')?.onEvent({
        kind: 'snapshot',
        snapshot: aircraftSnapshot([], { generatedAt: '2026-10-11T12:00:31Z' }),
      });
    });
    expect(screen.getByText('No longer in feed · last seen 12:00:01 UTC')).toBeInTheDocument();
    // The last known record stays readable.
    expect(screen.getByRole('region', { name: 'SAS1462' })).toBeInTheDocument();
  });

  it('offers to turn the layer on when the selection is in a layer that is off', async () => {
    harness = await startHarness({ catalog: CATALOG, globe: true });
    const h = harness;
    renderWithRuntime(h.runtime, <Inspector />);
    select(h, 'aircraft', 'abc123');
    expect(
      screen.getByText('abc123 is in Aircraft, which is off. Turn it on to see it.'),
    ).toBeVisible();
    await userEvent.setup().click(screen.getByRole('button', { name: 'Turn on Aircraft' }));
    expect(h.runtime.stores.workspace.getState().layers.aircraft?.enabled).toBe(true);
    // While the feed connects the inspector waits rather than claiming the entity is gone.
    expect(screen.getByText('Waiting for Aircraft data…')).toBeVisible();
    act(() => {
      h.platform.live('aircraft')?.onEvent({ kind: 'snapshot', snapshot: aircraftSnapshot([]) });
    });
    expect(
      screen.getByText('abc123 is not in the latest Aircraft data for the area the globe shows.'),
    ).toBeVisible();
  });

  it('closes with the close button or Escape', async () => {
    const h = await setup('aircraft');
    h.platform.live('aircraft')?.onEvent({
      kind: 'snapshot',
      snapshot: aircraftSnapshot([aircraft('4aca2d', { callsign: 'SAS1462' })]),
    });
    select(h, 'aircraft', '4aca2d');
    await h.user.click(screen.getByRole('button', { name: 'Close inspector' }));
    expect(h.runtime.stores.workspace.getState().selection).toBeNull();
    expect(inspector()).toBeNull();

    select(h, 'aircraft', '4aca2d');
    screen.getByRole('button', { name: 'Fly to' }).focus();
    await h.user.keyboard('{Escape}');
    await waitFor(() => expect(inspector()).toBeNull());
    expect(h.runtime.stores.workspace.getState().selection).toBeNull();
  });
});
