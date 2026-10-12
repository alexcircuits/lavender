import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { IslandBoundary } from './IslandBoundary';

let broken = true;
function Fragile() {
  if (broken) throw new Error('bad data');
  return <p>Recovered</p>;
}

describe('IslandBoundary', () => {
  it('contains a failure to its island and recovers on request', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    broken = true;
    const { rerender } = render(
      <IslandBoundary title="Layers" resetKey="layers">
        <Fragile />
      </IslandBoundary>,
    );
    expect(screen.getByRole('alert')).toHaveTextContent('Layers stopped working');

    broken = false;
    await userEvent.setup().click(screen.getByRole('button', { name: 'Try again' }));
    expect(screen.getByText('Recovered')).toBeInTheDocument();

    // A different panel or selection starts clean even after a failure.
    broken = true;
    rerender(
      <IslandBoundary title="Layers" resetKey="layers-2">
        <Fragile />
      </IslandBoundary>,
    );
    expect(screen.getByRole('alert')).toBeInTheDocument();
    broken = false;
    rerender(
      <IslandBoundary title="Sources" resetKey="sources">
        <Fragile />
      </IslandBoundary>,
    );
    expect(screen.getByText('Recovered')).toBeInTheDocument();
  });
});
