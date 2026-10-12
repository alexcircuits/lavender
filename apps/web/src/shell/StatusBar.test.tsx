import { act, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { type Harness, renderWithRuntime, startHarness } from '../app/testing';
import { formatCount } from '../design';
import { catalogOf, layer, quakeSnapshot } from '../test/fakes';
import { formatHeight, StatusBar } from './StatusBar';

let harness: Harness | null = null;

async function setup() {
  harness = await startHarness({
    catalog: catalogOf([layer('earthquakes', { name: 'Earthquakes' }), layer('aircraft')]),
    globe: true,
  });
  renderWithRuntime(harness.runtime, <StatusBar />);
  return harness;
}

afterEach(() => {
  harness?.stop();
  harness = null;
});

describe('StatusBar', () => {
  it('states the transport in words', async () => {
    const h = await setup();
    expect(screen.getByText('Connected')).toBeInTheDocument();
    act(() => h.platform.setTransport('polling'));
    expect(screen.getByText('Live stream down · polling')).toBeInTheDocument();
    act(() => h.platform.setTransport('offline'));
    expect(screen.getByText('Not connected')).toBeInTheDocument();
    act(() => h.platform.setTransport('connecting'));
    expect(screen.getByText('Connecting…')).toBeInTheDocument();
  });

  it('counts the feeds that are live', async () => {
    const h = await setup();
    expect(screen.getByText(/No feeds on/)).toBeInTheDocument();
    act(() => h.runtime.stores.workspace.getState().setLayerEnabled('earthquakes', true));
    expect(screen.getByText(/0 feeds live/)).toBeInTheDocument();
    act(() => {
      h.platform.live('earthquakes')?.onEvent({ kind: 'snapshot', snapshot: quakeSnapshot(['a']) });
    });
    expect(screen.getByText(/1 feed live/)).toBeInTheDocument();
  });

  it('shows the pointer position in decimal degrees and MGRS', async () => {
    const h = await setup();
    act(() => h.globe?.cursor({ lat: 38.897676, lon: -77.036548 }));
    expect(screen.getByTestId('cursor-position')).toHaveTextContent(
      'Pointer at 38.8977° N, 77.0365° W · 18SUJ 23390 07393',
    );
    act(() => h.globe?.cursor({ lat: 85, lon: 10 }));
    expect(screen.getByTestId('cursor-position')).toHaveTextContent('85.0000° N, 10.0000° E');
    act(() => h.globe?.cursor(null));
    expect(screen.queryByTestId('cursor-position')).not.toBeInTheDocument();
  });

  it('shows the camera height and the UTC clock', async () => {
    const h = await setup();
    act(() => h.globe?.view({ bbox: null, centre: null, heightM: 12_400_000 }));
    // Testing Library normalises the narrow no-break space in "12 400" to a plain space.
    expect(screen.getByText('Camera at 12 400 km')).toBeInTheDocument();
    expect(screen.getByText(/^\d\d:\d\d:\d\d UTC$/)).toBeInTheDocument();
  });

  it('writes heights with sensible units', () => {
    expect(formatHeight(850)).toBe('850 m');
    expect(formatHeight(12_400)).toBe('12.4 km');
    expect(formatHeight(12_400_000)).toBe(`${formatCount(12_400)} km`);
  });
});
