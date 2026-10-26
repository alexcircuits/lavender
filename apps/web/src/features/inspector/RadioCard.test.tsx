import { fireEvent, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { type Harness, renderWithRuntime, startHarness } from '../../app/testing';
import type { RadioStation } from '../../contracts';
import { RadioCard, radioFacts } from './RadioCard';

const STATION: RadioStation = {
  id: 'abc-123',
  name: 'Harbor Dispatch',
  streamUrl: 'https://example.com/stream',
  countryCode: 'US',
  state: 'Washington',
  tags: ['fire', 'police'],
  category: 'emergency',
  codec: 'MP3',
  bitrateKbps: 64,
  votes: 12,
  clickCount: 345,
  position: { lat: 47.6062, lon: -122.3321 },
};

describe('RadioCard', () => {
  let harness: Harness | null = null;

  afterEach(() => {
    harness?.stop();
    harness = null;
  });

  it('formats optional facts only when present', () => {
    const facts = radioFacts(STATION);
    expect(facts).toEqual(
      expect.arrayContaining([
        { label: 'Category', value: 'Emergency' },
        { label: 'Stream', value: 'MP3 · 64 kbps', mono: true },
        { label: 'Region', value: 'Washington' },
        { label: 'Tags', value: 'fire, police' },
      ]),
    );
    expect(facts.some((f) => f.label === 'Language')).toBe(false);
    expect(facts.some((f) => f.label === 'Homepage')).toBe(false);
  });

  it('renders metrics and actions', async () => {
    harness = await startHarness({ globe: true });
    renderWithRuntime(harness.runtime, <RadioCard station={STATION} />);
    expect(screen.getAllByText('Harbor Dispatch').length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText('345')).toBeDefined();
    expect(screen.getByRole('button', { name: /Copy coordinates/i })).toBeDefined();
    fireEvent.click(screen.getByRole('button', { name: /Fly to/i }));
  });
});
