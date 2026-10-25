import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { type Harness, renderWithRuntime, startHarness } from '../../app/testing';
import {
  DirectionsPanel,
  formatDistance,
  formatDuration,
  parseCoordinates,
} from './DirectionsPanel';

const harnesses: Harness[] = [];

async function setup() {
  const h = await startHarness({ globe: true });
  harnesses.push(h);
  const view = renderWithRuntime(h.runtime, <DirectionsPanel />);
  return { h, view, user: userEvent.setup() };
}

afterEach(() => {
  for (const h of harnesses.splice(0)) h.stop();
});

describe('DirectionsPanel', () => {
  describe('formatting helpers', () => {
    it('formats distances correctly in meters and km/mi', () => {
      expect(formatDistance(500)).toBe('500 m');
      expect(formatDistance(1500)).toBe('1.5 km (0.9 mi)');
      expect(formatDistance(12500)).toBe('12.5 km (7.8 mi)');
    });

    it('formats durations in seconds, minutes, and hours', () => {
      expect(formatDuration(45)).toBe('45 s');
      expect(formatDuration(180)).toBe('3 min');
      expect(formatDuration(3660)).toBe('1 h 1 min');
      expect(formatDuration(7200)).toBe('2 h 0 min');
    });

    it('parses valid geographic coordinates in lon, lat format', () => {
      expect(parseCoordinates('13.4, 52.5')).toEqual([13.4, 52.5]);
      expect(parseCoordinates('-122.4194, 37.7749')).toEqual([-122.4194, 37.7749]);
      expect(parseCoordinates('invalid')).toBeNull();
      expect(parseCoordinates('200, 200')).toBeNull();
    });
  });

  it('renders initial state with empty state prompt and travel profile selector', async () => {
    await setup();
    expect(screen.getByRole('heading', { name: 'Directions' })).toBeInTheDocument();
    expect(screen.getByRole('radiogroup', { name: 'Travel profile' })).toBeInTheDocument();
    expect(screen.getByText('No active route')).toBeInTheDocument();
  });

  it('calculates a route, renders distance/duration, and updates globe', async () => {
    const { h, user } = await setup();
    const findRouteBtn = screen.getByRole('button', { name: 'Find Route' });

    await user.click(findRouteBtn);

    await waitFor(() => {
      expect(h.platform.calculateRoute).toHaveBeenCalledWith(
        'car',
        '13.3888,52.517;13.405,52.52',
        true,
      );
    });

    await waitFor(() => {
      expect(screen.getByText('1.0 km (0.6 mi)')).toBeInTheDocument();
      expect(screen.getByText('10 min')).toBeInTheDocument();
      expect(screen.getByText('Depart north')).toBeInTheDocument();
      expect(screen.getByText('Arrive at destination')).toBeInTheDocument();
    });

    expect(h.globe?.globe.setRoute).toHaveBeenCalled();
    expect(h.globe?.globe.setRouteWaypoints).toHaveBeenCalled();
  });

  it('handles maneuver step selection and camera flight', async () => {
    const { h, user } = await setup();
    const findRouteBtn = screen.getByRole('button', { name: 'Find Route' });
    await user.click(findRouteBtn);

    await waitFor(() => {
      expect(screen.getByText('Depart north')).toBeInTheDocument();
    });

    const stepBtn = screen.getByText('Depart north').closest('button');
    expect(stepBtn).not.toBeNull();
    if (stepBtn) {
      await user.click(stepBtn);
      expect(h.globe?.globe.selectRouteStep).toHaveBeenCalledWith(0);
      expect(h.globe?.globe.flyTo).toHaveBeenCalled();
    }
  });

  it('swaps origin and destination inputs', async () => {
    const { user } = await setup();
    const origin = screen.getByLabelText('Origin coordinates') as HTMLInputElement;
    const dest = screen.getByLabelText('Destination coordinates') as HTMLInputElement;

    const initialOrigin = origin.value;
    const initialDest = dest.value;

    const swapBtn = screen.getByRole('button', { name: 'Swap' });
    await user.click(swapBtn);

    expect(origin.value).toBe(initialDest);
    expect(dest.value).toBe(initialOrigin);
  });

  it('clears active route and resets globe', async () => {
    const { h, user } = await setup();
    const findRouteBtn = screen.getByRole('button', { name: 'Find Route' });
    await user.click(findRouteBtn);

    await waitFor(() => {
      expect(screen.getByText('1.0 km (0.6 mi)')).toBeInTheDocument();
    });

    const clearBtn = screen.getByRole('button', { name: 'Clear' });
    await user.click(clearBtn);

    expect(screen.getByText('No active route')).toBeInTheDocument();
    expect(h.globe?.globe.setRoute).toHaveBeenCalledWith(null);
  });

  it('displays error state when route calculation fails', async () => {
    const { h, user } = await setup();
    h.platform.calculateRoute.mockRejectedValueOnce(new Error('route leg too long'));

    const findRouteBtn = screen.getByRole('button', { name: 'Find Route' });
    await user.click(findRouteBtn);

    await waitFor(() => {
      expect(screen.getByText('Routing unavailable')).toBeInTheDocument();
      expect(screen.getByText('route leg too long')).toBeInTheDocument();
    });
  });
});
