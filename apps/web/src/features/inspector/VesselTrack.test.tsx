import { act, screen, waitFor, within } from '@testing-library/react';
import { useState } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { type Harness, renderWithRuntime, startHarness } from '../../app/testing';
import type { VesselTrackResponse, VesselTrackSample } from '../../contracts';
import { summariseTrack, VesselTrack } from './VesselTrack';

/** Synthetic positions (invented ship, no live AIS data). */
const at = (epochSec: number, lon = 24.9): VesselTrackSample => ({ lat: 60.1, lon, epochSec });
const T0 = Date.parse('2026-10-15T11:50:00Z') / 1000;

function trackOf(samples: VesselTrackSample[]): VesselTrackResponse {
  return { mmsi: '230990001', samples, source: 'aisstream', retainedSec: 1_800 };
}

let harness: Harness | null = null;

afterEach(() => {
  harness?.stop();
  harness = null;
  vi.restoreAllMocks();
});

describe('summariseTrack', () => {
  it('drops unusable and repeated samples, lists newest first and bounds the rows', () => {
    const summary = summariseTrack(
      trackOf([at(T0), at(T0 + 60), at(T0 + 60), at(T0 + 30, Number.NaN), at(T0 + 300)]),
    );
    expect(summary.count).toBe(3);
    expect(summary.places).toBe(1);
    expect(summary.spanSec).toBe(300);
    expect(summary.rows.map((s) => s.epochSec)).toEqual([T0 + 300, T0 + 60, T0]);
  });

  it('summarises a very long track without spreading it into arguments', () => {
    const samples = Array.from({ length: 300_000 }, (_, i) => at(T0 + i, 24 + i / 1e6));
    const summary = summariseTrack(trackOf(samples));
    expect(summary.count).toBe(300_000);
    expect(summary.spanSec).toBe(299_999);
    expect(summary.rows).toHaveLength(1_000);
    expect(summary.rows[0]?.epochSec).toBe(T0 + 299_999);
  });

  it('treats a malformed response as an empty track', () => {
    const summary = summariseTrack({ ...trackOf([]), samples: null as unknown as [] });
    expect(summary).toEqual({ rows: [], count: 0, places: 0, spanSec: 0 });
  });
});

describe('VesselTrack', () => {
  it('shows repeated samples once, without duplicate row keys', async () => {
    harness = await startHarness({ globe: true });
    const errors = vi.spyOn(console, 'error');
    harness.platform.vesselTrack.mockResolvedValue(trackOf([at(T0), at(T0), at(T0 + 120)]));
    renderWithRuntime(harness.runtime, <VesselTrack mmsi="230990001" />);
    expect(await screen.findByText(/^2 positions over 2 min/)).toBeInTheDocument();
    const table = screen.getByRole('table', { name: 'Vessel track' });
    expect(within(table).getAllByRole('row')).toHaveLength(3);
    expect(
      errors.mock.calls.some((call) => String(call[0]).includes('Encountered two children')),
    ).toBe(false);
  });

  it('only promises a trail on the globe when the ship has reported from two places', async () => {
    harness = await startHarness({ globe: true });
    // At anchor: two reports from the same place draw no line.
    harness.platform.vesselTrack.mockResolvedValue(
      trackOf([at(T0), at(T0 + 300), null as unknown as VesselTrackSample]),
    );
    const { unmount } = renderWithRuntime(harness.runtime, <VesselTrack mmsi="276990003" />);
    expect(await screen.findByText(/^2 positions over 5 min/)).toBeInTheDocument();
    expect(screen.getByText(/all from one place, so no trail is drawn/)).toBeInTheDocument();
    expect(screen.queryByText(/drawn on the globe/)).not.toBeInTheDocument();
    unmount();

    harness.platform.vesselTrack.mockResolvedValue(trackOf([at(T0), at(T0 + 60, 24.91)]));
    renderWithRuntime(harness.runtime, <VesselTrack mmsi="230990001" />);
    expect(
      await screen.findByText(/drawn on the globe as the selection trail/),
    ).toBeInTheDocument();
  });

  it('shows the voyage details that come with the track, unknown ones as not reported', async () => {
    harness = await startHarness({ globe: true });
    harness.platform.vesselTrack.mockResolvedValue({
      ...trackOf([at(T0), at(T0 + 60)]),
      voyage: { callSign: 'XTEST', eta: { month: 10, day: 15, hour: 18, minute: 0 } },
    });
    renderWithRuntime(harness.runtime, <VesselTrack mmsi="230990001" />);
    const voyage = await screen.findByLabelText('Voyage', { selector: 'dl' });
    const value = (label: string) =>
      within(voyage).getByText(label, { selector: 'dt' }).nextElementSibling?.textContent;
    expect(value('Call sign')).toBe('XTEST');
    expect(value('ETA')).toBe('15 Oct 18:00 UTC');
    expect(value('Navigational status')).toBe('Not reported');
    expect(value('Length × beam')).toBe('Not reported');
  });

  it('reloads the track when the ship reports a newer position, at most every 30 s', async () => {
    harness = await startHarness({ globe: true });
    const h = harness;
    h.platform.vesselTrack.mockResolvedValue(trackOf([at(T0), at(T0 + 60)]));
    let report: (epochSec: number) => void = () => undefined;
    function Selected() {
      const [reportedAtSec, setReportedAtSec] = useState(T0 + 60);
      report = setReportedAtSec;
      return <VesselTrack mmsi="230990001" reportedAtSec={reportedAtSec} />;
    }
    renderWithRuntime(h.runtime, <Selected />);
    await waitFor(() => expect(h.platform.vesselTrack).toHaveBeenCalledTimes(1));
    // Reports within the same 30 s of report time do not refetch.
    await act(async () => report(T0 + 70));
    expect(h.platform.vesselTrack).toHaveBeenCalledTimes(1);
    h.platform.vesselTrack.mockResolvedValue(trackOf([at(T0), at(T0 + 60), at(T0 + 120)]));
    await act(async () => report(T0 + 120));
    await waitFor(() => expect(h.platform.vesselTrack).toHaveBeenCalledTimes(2));
    expect(await screen.findByText(/^3 positions over 2 min/)).toBeInTheDocument();
    expect(h.globe?.globe.showVesselTrack).toHaveBeenLastCalledWith(
      expect.objectContaining({ samples: expect.arrayContaining([at(T0 + 120)]) }),
    );
  });
});
