import { act, fireEvent, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { type Harness, renderWithRuntime, startHarness } from '../../app/testing';
import { ReplayPanel } from './ReplayPanel';

const harnesses: Harness[] = [];

async function setup() {
  const h = await startHarness({ globe: true });
  harnesses.push(h);
  const view = renderWithRuntime(h.runtime, <ReplayPanel />);
  return { h, view, user: userEvent.setup() };
}

afterEach(() => {
  for (const h of harnesses.splice(0)) h.stop();
});

describe('ReplayPanel', () => {
  it('shows empty prompt when no entity is selected', async () => {
    await setup();
    expect(screen.getByText(/No replayable entity selected/i)).toBeInTheDocument();
  });

  it('loads aircraft track when an aircraft is selected, and renders trail on globe', async () => {
    const { h } = await setup();

    h.platform.aircraftTrack.mockResolvedValueOnce({
      hex: 'a1b2c3',
      points: [
        {
          time: '2026-10-25T10:00:00Z',
          position: { lat: 37.77, lon: -122.41, altM: 3000 },
        },
        {
          time: '2026-10-25T10:05:00Z',
          position: { lat: 37.8, lon: -122.45, altM: 3500 },
        },
        {
          time: '2026-10-25T10:10:00Z',
          position: { lat: 37.85, lon: -122.5, altM: 4000 },
        },
      ],
      sources: ['opensky'],
    });

    act(() => {
      h.runtime.stores.workspace.getState().select({ feed: 'aircraft', id: 'a1b2c3' });
    });

    await waitFor(() => {
      expect(h.platform.aircraftTrack).toHaveBeenCalledWith('a1b2c3', expect.anything());
    });

    // Globe should draw the trail
    await waitFor(() => {
      expect(h.globe?.globe.showAircraftTrack).toHaveBeenCalledWith(
        expect.objectContaining({ hex: 'a1b2c3' }),
      );
    });

    // Scrubber slider and telemetry should be rendered
    expect(await screen.findByLabelText(/Replay timeline scrubber/i)).toBeInTheDocument();
    expect(screen.getByText(/3 positions retained/i)).toBeInTheDocument();
    expect(screen.getByText(/37.77000/i)).toBeInTheDocument();
    expect(screen.getByText(/-122.41000/i)).toBeInTheDocument();
  });

  it('loads vessel track when a vessel is selected', async () => {
    const { h } = await setup();

    h.platform.vesselTrack.mockResolvedValueOnce({
      mmsi: '987654321',
      samples: [
        { epochSec: 1700000000, lat: 25.0, lon: -71.0 },
        { epochSec: 1700000600, lat: 25.1, lon: -71.1 },
        { epochSec: 1700001200, lat: 25.2, lon: -71.2 },
      ],
      source: 'aisstream',
      retainedSec: 3600,
    });

    act(() => {
      h.runtime.stores.workspace.getState().select({ feed: 'vessels', id: '987654321' });
    });

    await waitFor(() => {
      expect(h.platform.vesselTrack).toHaveBeenCalledWith('987654321', expect.anything());
    });

    await waitFor(() => {
      expect(h.globe?.globe.showVesselTrack).toHaveBeenCalledWith(
        expect.objectContaining({ mmsi: '987654321' }),
      );
    });

    expect(await screen.findByLabelText(/Replay timeline scrubber/i)).toBeInTheDocument();
    expect(screen.getByText(/3 positions retained/i)).toBeInTheDocument();
  });

  it('allows seeking, changing playback speed, and play/pause toggle', async () => {
    const { h, user } = await setup();

    h.platform.aircraftTrack.mockResolvedValueOnce({
      hex: 'test123',
      points: [
        {
          time: '2026-10-25T10:00:00Z',
          position: { lat: 40.0, lon: -74.0, altM: 1000 },
        },
        {
          time: '2026-10-25T10:10:00Z',
          position: { lat: 40.2, lon: -74.2, altM: 2000 },
        },
      ],
      sources: ['adsb_lol'],
    });

    act(() => {
      h.runtime.stores.workspace.getState().select({ feed: 'aircraft', id: 'test123' });
    });

    const scrubber = await screen.findByLabelText(/Replay timeline scrubber/i);

    // Seek scrubber forward
    const startTime = Math.floor(new Date('2026-10-25T10:00:00Z').getTime() / 1000);
    const midTime = startTime + 300;
    fireEvent.change(scrubber, { target: { value: midTime } });

    // Telemetry updates to interpolated midway point
    expect(screen.getByText('40.10000')).toBeInTheDocument();
    expect(screen.getByText('-74.10000')).toBeInTheDocument();
    expect(screen.getByText('1500 m')).toBeInTheDocument();

    // Toggle playback speed
    const speed2x = screen.getByRole('button', { name: '2×' });
    await user.click(speed2x);
    expect(speed2x).toHaveAttribute('aria-pressed', 'true');

    // Play/Pause button
    const playBtn = screen.getByLabelText(/Play replay/i);
    await user.click(playBtn);
    expect(screen.getByLabelText(/Pause replay/i)).toBeInTheDocument();
  });

  it('triggers camera follow when enabled', async () => {
    const { h, user } = await setup();

    h.platform.aircraftTrack.mockResolvedValueOnce({
      hex: 'cameraHex',
      points: [
        {
          time: '2026-10-25T12:00:00Z',
          position: { lat: 51.5, lon: -0.1, altM: 1500 },
        },
        {
          time: '2026-10-25T12:05:00Z',
          position: { lat: 51.52, lon: -0.12, altM: 1800 },
        },
      ],
      sources: ['opensky'],
    });

    act(() => {
      h.runtime.stores.workspace.getState().select({ feed: 'aircraft', id: 'cameraHex' });
    });

    await screen.findByLabelText(/Replay timeline scrubber/i);

    const followCheckbox = screen.getByRole('checkbox', { name: /Follow entity with camera/i });
    expect(followCheckbox).not.toBeChecked();

    await user.click(followCheckbox);
    expect(followCheckbox).toBeChecked();

    await waitFor(() => {
      expect(h.globe?.globe.flyTo).toHaveBeenCalledWith(
        expect.objectContaining({
          kind: 'point',
          position: expect.objectContaining({ lat: 51.5, lon: -0.1 }),
        }),
      );
    });
  });
});
