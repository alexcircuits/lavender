import { fireEvent, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { type Harness, renderWithRuntime, startHarness } from '../../app/testing';
import { GnssCard, type GnssCell, gnssFacts } from './GnssCard';

const HIGH_CELL: GnssCell = {
  id: '280:400',
  bbox: { west: 20, south: 50, east: 20.5, north: 50.5 },
  center: { lat: 50.25, lon: 20.25 },
  aircraft: 12,
  degraded: 5,
  percentDegraded: 33.3,
  level: 'high',
  state: 'navigation-integrity-anomaly',
  interpretation: 'suspected-gnss-interference',
};

const LOW_CELL: GnssCell = {
  id: '160:340',
  bbox: { west: -10, south: -10, east: -9.5, north: -9.5 },
  center: { lat: -9.75, lon: -9.75 },
  aircraft: 8,
  degraded: 1,
  percentDegraded: 0,
  level: 'low',
  state: 'nominal',
};

describe('GnssCard', () => {
  let harness: Harness | null = null;

  afterEach(() => {
    harness?.stop();
    harness = null;
    vi.restoreAllMocks();
  });

  it('formats GNSS integrity facts accurately for anomaly and nominal cells', () => {
    const highFacts = gnssFacts(HIGH_CELL);
    expect(highFacts).toEqual(
      expect.arrayContaining([
        { label: '0.5° × 0.5° Cell', value: '280:400', mono: true },
        { label: 'Severity Level', value: 'HIGH', mono: true },
        { label: 'Degraded Ratio', value: '5 / 12 aircraft', mono: true },
        { label: 'Discounted Anomaly Rate', value: '33.3%', mono: true },
        { label: 'Interpretation', value: 'Suspected GNSS interference' },
        {
          label: 'DO-260A/B Classifier',
          value: 'NIC < 7 or NACp < 8 with 1-aircraft false-positive discount',
        },
      ]),
    );

    const lowFacts = gnssFacts(LOW_CELL);
    expect(lowFacts).toEqual(
      expect.arrayContaining([
        { label: '0.5° × 0.5° Cell', value: '160:340', mono: true },
        { label: 'Severity Level', value: 'LOW', mono: true },
        { label: 'Degraded Ratio', value: '1 / 8 aircraft', mono: true },
        { label: 'Discounted Anomaly Rate', value: '0.0%', mono: true },
        { label: 'Interpretation', value: 'Nominal navigation accuracy' },
      ]),
    );
  });

  it('renders cell ID, severity badge, degraded ratio, anomaly rate, rule note, and Fly to cell button', async () => {
    harness = await startHarness({ globe: true });
    renderWithRuntime(harness.runtime, <GnssCard cell={HIGH_CELL} />);

    expect(screen.getAllByText(/280:400/).length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText('HIGH').length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText('5 / 12 aircraft')).toBeDefined();
    expect(screen.getAllByText('33.3%').length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText(/Suspected GNSS interference/).length).toBeGreaterThanOrEqual(1);
    expect(
      screen.getAllByText(/NIC < 7 or NACp < 8 with 1-aircraft false-positive discount/).length,
    ).toBeGreaterThanOrEqual(1);

    const flyBtn = screen.getByRole('button', { name: /Fly to cell/i });
    expect(flyBtn).toBeDefined();
    fireEvent.click(flyBtn);
  });

  it('renders nominal interpretation for low cells', async () => {
    harness = await startHarness({ globe: true });
    renderWithRuntime(harness.runtime, <GnssCard cell={LOW_CELL} />);

    expect(screen.getAllByText('LOW').length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText(/Nominal navigation accuracy/).length).toBeGreaterThanOrEqual(1);
  });
});
