import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { noaaRecordUrl } from '../../lib/safeUrl';
import { cyclone } from '../../test/cyclone';
import { CycloneForecast } from './CycloneForecast';
import { cycloneFacts, inspect } from './facts';

describe('cyclone inspection', () => {
  it('shows every current observation field and forecast wind/position accessibly', () => {
    const storm = cyclone();
    expect(inspect('cyclones', storm.id, storm)?.kind).toBe('cyclones');
    expect(inspect('cyclones', 'other', storm)).toBeNull();
    const facts = cycloneFacts(storm);
    expect(facts).toContainEqual({ label: 'Maximum sustained wind', value: '70 kn' });
    expect(facts).toContainEqual({ label: 'Central pressure', value: '980 mb' });
    expect(facts).toContainEqual({ label: 'Basin', value: 'Atlantic' });
    render(<CycloneForecast storm={storm} />);
    expect(
      screen.getByRole('table', { name: 'Advisory 6 forecast positions' }),
    ).toBeInTheDocument();
    expect(screen.getByRole('rowheader', { name: '12 h' })).toBeInTheDocument();
    expect(screen.getByText('75 kn')).toBeInTheDocument();
    expect(screen.getByText('90 kn')).toBeInTheDocument();
  });
  it('keeps missing intensity unknown and never displays unconfirmed forecasts', () => {
    const storm = cyclone({ geometryStatus: 'pending', intensityKt: undefined });
    expect(cycloneFacts(storm).find((f) => f.label === 'Maximum sustained wind')?.value).toBe(
      'Not reported',
    );
    render(<CycloneForecast storm={storm} />);
    expect(screen.queryByRole('table')).not.toBeInTheDocument();
    expect(screen.getByRole('status')).toHaveTextContent('pending');
  });
  it('allows only official credential-free HTTPS records', () => {
    expect(noaaRecordUrl('https://www.nhc.noaa.gov/text/MIATCPAT1.shtml')).toBeTruthy();
    for (const url of [
      'javascript:alert(1)',
      'https://noaa.gov.evil.test/x',
      'http://www.nhc.noaa.gov/x',
      'https://secret@www.nhc.noaa.gov/x',
    ])
      expect(noaaRecordUrl(url)).toBeNull();
  });
});
