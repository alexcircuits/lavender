import { screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { type Harness, renderWithRuntime, startHarness } from '../../app/testing';
import type { CanonicalEntity } from '../../contracts';
import { EntityResolutionCard } from './EntityResolutionCard';

const SAMPLE_CANONICAL: CanonicalEntity = {
  id: 'canonical:aircraft:4ca1b2',
  kind: 'aircraft',
  primaryLabel: 'RYR12AB',
  identifiers: [
    { scheme: 'icao24', value: '4ca1b2' },
    { scheme: 'callsign', value: 'RYR12AB' },
    { scheme: 'registration', value: 'EI-DCL' },
  ],
  attributes: {
    typeCode: 'B738',
    isMilitary: 'false',
  },
  firstSeen: 1_729_000_000n,
  lastSeen: 1_729_003_600n,
  confidence: 0.98,
};

describe('EntityResolutionCard', () => {
  let harness: Harness | null = null;

  afterEach(() => {
    harness?.stop();
    harness = null;
    vi.restoreAllMocks();
  });

  it('renders resolved canonical identity and alias tags', async () => {
    harness = await startHarness();
    vi.spyOn(harness.platform, 'resolveEntity').mockResolvedValue(SAMPLE_CANONICAL);

    renderWithRuntime(harness.runtime, <EntityResolutionCard feed="aircraft" entityId="4ca1b2" />);

    await waitFor(() => {
      expect(screen.getByText('Canonical Identity')).toBeInTheDocument();
    });

    expect(screen.getAllByText('RYR12AB').length).toBeGreaterThanOrEqual(2);
    expect(screen.getByText('canonical:aircraft:4ca1b2')).toBeInTheDocument();
    expect(screen.getByText('98% deterministic')).toBeInTheDocument();
    expect(screen.getByText('4ca1b2')).toBeInTheDocument();
    expect(screen.getByText('EI-DCL')).toBeInTheDocument();
  });

  it('returns null for unresolvable feeds', async () => {
    harness = await startHarness();
    renderWithRuntime(
      harness.runtime,
      <EntityResolutionCard feed="earthquakes" entityId="eq-123" />,
    );
    expect(screen.queryByLabelText('Entity Resolution')).toBeNull();
  });
});
