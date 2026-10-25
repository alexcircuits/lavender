import { screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { type Harness, renderWithRuntime, startHarness } from '../../app/testing';
import type { AirQualityReading } from '../../contracts';
import { AirQualityCard, airQualityFacts } from './AirQualityCard';

const SAMPLE_READING: AirQualityReading = {
  id: 'eccc:station_01',
  provider: 'eccc',
  scale: 'AQHI',
  value: 3,
  band: 'low',
  stationId: 'station_01',
  name: 'Downtown Observation Station',
  zone: 'ont',
  latitude: 43.6532,
  longitude: -79.3832,
  observedAt: '2026-10-25T14:00:00Z',
  color: '#00ccff',
  label: '3',
  risk: 'Low Risk',
  note: 'Ideal air quality for outdoor activities.',
};

describe('AirQualityCard', () => {
  let harness: Harness | null = null;

  afterEach(() => {
    harness?.stop();
    harness = null;
    vi.restoreAllMocks();
  });

  it('formats facts accurately from reading data', () => {
    const facts = airQualityFacts(SAMPLE_READING);
    expect(facts).toEqual(
      expect.arrayContaining([
        { label: 'Station Name', value: 'Downtown Observation Station' },
        { label: 'Index Value', value: '3 (AQHI)', mono: true },
        { label: 'Health Category', value: 'Low Health Risk (1–3)' },
        { label: 'Risk Assessment', value: 'Low Risk' },
        { label: 'Health Advice', value: 'Ideal air quality for outdoor activities.' },
        { label: 'Forecast Zone', value: 'ONT' },
        { label: 'Station ID', value: 'station_01', mono: true },
        { label: 'Data Source', value: 'ECCC' },
      ]),
    );
  });

  it('renders station title and category summary', async () => {
    harness = await startHarness({ globe: true });
    renderWithRuntime(harness.runtime, <AirQualityCard reading={SAMPLE_READING} />);
    expect(screen.getAllByText('Downtown Observation Station').length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText(/AQHI 3 · Low Health Risk/)).toBeDefined();
  });
});
