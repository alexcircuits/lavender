import { fireEvent, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { type Harness, renderWithRuntime, startHarness } from '../../app/testing';
import type { AlprCamera } from '../../contracts';
import { AlprCard, alprFacing, alprFacts } from './AlprCard';

const CAMERA: AlprCamera = {
  id: 'alpr:42',
  osmId: 42n,
  position: { lat: 40.7128, lon: -74.006 },
  operator: 'City PD',
  manufacturer: 'Flock Safety',
  directionDeg: 92,
};

describe('AlprCard', () => {
  let harness: Harness | null = null;

  afterEach(() => {
    harness?.stop();
    harness = null;
  });

  it('formats facing and facts', () => {
    expect(alprFacing(92)).toBe('E (92°)');
    expect(alprFacing(undefined)).toBe('Unspecified');
    const facts = alprFacts(CAMERA);
    expect(facts).toEqual(
      expect.arrayContaining([
        { label: 'Operator', value: 'City PD' },
        { label: 'OSM Node', value: '42', mono: true },
      ]),
    );
    expect(facts.some((f) => f.label === 'Zone')).toBe(false);
  });

  it('renders hero metrics and actions', async () => {
    harness = await startHarness({ globe: true });
    renderWithRuntime(harness.runtime, <AlprCard camera={CAMERA} />);
    expect(screen.getAllByText('E (92°)').length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText('Flock Safety').length).toBeGreaterThanOrEqual(1);
    expect(screen.getByRole('button', { name: /Copy coordinates/i })).toBeDefined();
    fireEvent.click(screen.getByRole('button', { name: /Fly to/i }));
  });
});
