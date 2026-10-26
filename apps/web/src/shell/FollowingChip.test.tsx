import { act, fireEvent, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { type Harness, renderWithRuntime, startHarness } from '../app/testing';
import type { TrackingState } from '../globe';
import { catalogOf, layer } from '../test/fakes';
import { FollowingChip } from './FollowingChip';

let harness: Harness | null = null;

async function setup() {
  harness = await startHarness({
    catalog: catalogOf([layer('aircraft')]),
    globe: true,
  });
  return harness;
}

afterEach(() => {
  harness?.stop();
  harness = null;
});

describe('FollowingChip', () => {
  it('renders nothing when not following', async () => {
    const h = await setup();
    renderWithRuntime(h.runtime, <FollowingChip />);
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });

  it('renders "Following [ID] · Esc to stop" when actively tracking', async () => {
    const h = await setup();
    let followListener: ((state: TrackingState) => void) | null = null;
    if (h.globe) {
      h.globe.globe.onFollowChange = (listener: (state: TrackingState) => void) => {
        followListener = listener;
        return () => {
          followListener = null;
        };
      };
    }

    renderWithRuntime(h.runtime, <FollowingChip />);

    act(() => {
      followListener?.({
        status: 'tracking',
        mode: 'follow',
        target: { feed: 'aircraft', id: 'FIN123' },
        position: null,
        lastUpdateMs: Date.now(),
      });
    });

    expect(screen.getByText('Following FIN123 · Esc to stop')).toBeInTheDocument();
    expect(screen.queryByText(/Stale/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Lost/)).not.toBeInTheDocument();
  });

  it('shows badge "Stale (estimating)" when status is stale', async () => {
    const h = await setup();
    let followListener: ((state: TrackingState) => void) | null = null;
    if (h.globe) {
      h.globe.globe.onFollowChange = (listener: (state: TrackingState) => void) => {
        followListener = listener;
        return () => {
          followListener = null;
        };
      };
    }

    renderWithRuntime(h.runtime, <FollowingChip />);

    act(() => {
      followListener?.({
        status: 'stale',
        mode: 'follow',
        target: { feed: 'aircraft', id: 'FIN123' },
        position: null,
        lastUpdateMs: Date.now() - 35_000,
      });
    });

    expect(screen.getByText('Following FIN123 · Esc to stop')).toBeInTheDocument();
    expect(screen.getByText('Stale (estimating)')).toBeInTheDocument();
  });

  it('shows badge "Lost (last seen [X] ago)" when status is lost', async () => {
    const h = await setup();
    let followListener: ((state: TrackingState) => void) | null = null;
    if (h.globe) {
      h.globe.globe.onFollowChange = (listener: (state: TrackingState) => void) => {
        followListener = listener;
        return () => {
          followListener = null;
        };
      };
    }

    renderWithRuntime(h.runtime, <FollowingChip />);

    act(() => {
      followListener?.({
        status: 'lost',
        mode: 'follow',
        target: { feed: 'aircraft', id: 'FIN123' },
        position: null,
        lastUpdateMs: Date.now() - 65_000,
      });
    });

    expect(screen.getByText('Following FIN123 · Esc to stop')).toBeInTheDocument();
    expect(screen.getByText(/Lost \(last seen .+ ago\)/)).toBeInTheDocument();
  });

  it('stops following on chip click or Esc key press', async () => {
    const h = await setup();
    let followListener: ((state: TrackingState) => void) | null = null;
    if (h.globe) {
      h.globe.globe.onFollowChange = (listener: (state: TrackingState) => void) => {
        followListener = listener;
        return () => {
          followListener = null;
        };
      };
    }

    renderWithRuntime(h.runtime, <FollowingChip />);

    act(() => {
      followListener?.({
        status: 'tracking',
        mode: 'follow',
        target: { feed: 'aircraft', id: 'FIN123' },
        position: null,
        lastUpdateMs: Date.now(),
      });
    });

    const chip = screen.getByRole('button', { name: /Following FIN123/ });
    fireEvent.click(chip);
    expect(h.globe?.globe.stopFollowing).toHaveBeenCalledTimes(1);

    // Test Esc key
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(h.globe?.globe.stopFollowing).toHaveBeenCalledTimes(2);
  });

  it('shows "Cockpit [ID] · Esc to exit" when mode is cockpit', async () => {
    const h = await setup();
    let followListener: ((state: TrackingState) => void) | null = null;
    if (h.globe) {
      h.globe.globe.onFollowChange = (listener: (state: TrackingState) => void) => {
        followListener = listener;
        return () => {
          followListener = null;
        };
      };
    }

    renderWithRuntime(h.runtime, <FollowingChip />);

    act(() => {
      followListener?.({
        status: 'tracking',
        mode: 'cockpit',
        target: { feed: 'aircraft', id: 'AFR456' },
        position: null,
        lastUpdateMs: Date.now(),
      });
    });

    expect(screen.getByText('Cockpit AFR456 · Esc to exit')).toBeInTheDocument();
  });
});
