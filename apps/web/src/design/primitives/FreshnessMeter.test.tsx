import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { FreshnessMeter, freshnessState } from './FreshnessMeter';

describe('FreshnessMeter', () => {
  it('is a meter whose value is the age, on a 0–3× cadence scale, with the age in words', () => {
    render(<FreshnessMeter label="Aircraft" ageSeconds={42} cadenceSeconds={60} />);
    const meter = screen.getByRole('meter', { name: 'Aircraft freshness' });
    expect(meter).toHaveAttribute('aria-valuenow', '42');
    expect(meter).toHaveAttribute('aria-valuemin', '0');
    expect(meter).toHaveAttribute('aria-valuemax', '180');
    expect(meter).toHaveAttribute('aria-valuetext', '42 s old, updates every 1 min');
    expect(meter).toHaveAttribute('data-state', 'fresh');
  });

  it('turns to the warning state past twice the cadence', () => {
    render(<FreshnessMeter label="Satellites" ageSeconds={250} cadenceSeconds={120} />);
    const meter = screen.getByRole('meter', { name: 'Satellites freshness' });
    expect(meter).toHaveAttribute('data-state', 'late');
    expect(meter).toHaveAttribute('aria-valuetext', '4 min 10 s old, updates every 2 min');
    expect(freshnessState(240, 120)).toBe('fresh');
    expect(freshnessState(241, 120)).toBe('late');
  });

  it('shows unknown as unknown, never as zero', () => {
    render(<FreshnessMeter label="Fires" ageSeconds={null} cadenceSeconds={600} />);
    expect(screen.queryByRole('meter')).not.toBeInTheDocument();
    const unknown = screen.getByRole('img', { name: /Fires freshness: Age unknown/ });
    expect(unknown).toHaveAttribute('data-state', 'unknown');
  });

  it('states the exact age in a tooltip on hover', async () => {
    const user = userEvent.setup();
    render(<FreshnessMeter label="Earthquakes" ageSeconds={95} cadenceSeconds={60} />);
    await user.hover(screen.getByRole('meter'));
    const tip = await screen.findByText('1 min 35 s old, updates every 1 min');
    await waitFor(() => expect(tip).toBeVisible());
  });
});
