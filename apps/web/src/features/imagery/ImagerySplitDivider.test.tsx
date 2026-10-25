import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useUi } from '../../runtime';
import { ImagerySplitDivider } from './ImagerySplitDivider';
import { useImageryStore } from './imageryStore';

vi.mock('../../runtime', () => ({
  useUi: vi.fn(),
}));

describe('ImagerySplitDivider', () => {
  let mockGlobe: {
    setRecentImagerySplitPosition: ReturnType<typeof vi.fn>;
  };

  beforeEach(() => {
    useImageryStore.getState().clear();
    mockGlobe = {
      setRecentImagerySplitPosition: vi.fn(),
    };
    (useUi as unknown as ReturnType<typeof vi.fn>).mockImplementation((selector) =>
      selector({ globe: mockGlobe }),
    );
  });

  it('renders nothing when mode is "image"', () => {
    useImageryStore.setState({
      mode: 'image',
      box: { west: -97.8, south: 30.2, east: -97.7, north: 30.3 },
      slotA: {
        key: 'S30-2026-10-20',
        product: 'S30',
        day: '2026-10-20',
        cloud: null,
        timeRange: null,
        availability: 'present',
        coverage: 'full',
        granuleCount: 1,
      },
    });

    const { container } = render(<ImagerySplitDivider />);
    expect(container.firstChild).toBeNull();
  });

  it('renders split handle and badges when in basemap mode', () => {
    useImageryStore.setState({
      mode: 'basemap',
      box: { west: -97.8, south: 30.2, east: -97.7, north: 30.3 },
      slotA: {
        key: 'S30-2026-10-20',
        product: 'S30',
        day: '2026-10-20',
        cloud: null,
        timeRange: null,
        availability: 'present',
        coverage: 'full',
        granuleCount: 1,
      },
      splitPosition: 0.5,
    });

    render(<ImagerySplitDivider />);
    const slider = screen.getByRole('slider', { name: /Satellite imagery comparison split/i });
    expect(slider).toBeInTheDocument();
    expect(slider).toHaveAttribute('aria-valuenow', '50');
    expect(screen.getByText('S30 2026-10-20')).toBeInTheDocument();
    expect(screen.getByText('BASEMAP')).toBeInTheDocument();
  });

  it('responds to keyboard arrow navigation', () => {
    useImageryStore.setState({
      mode: 'ab',
      box: { west: -97.8, south: 30.2, east: -97.7, north: 30.3 },
      slotA: {
        key: 'S30-2026-10-20',
        product: 'S30',
        day: '2026-10-20',
        cloud: null,
        timeRange: null,
        availability: 'present',
        coverage: 'full',
        granuleCount: 1,
      },
      slotB: {
        key: 'L30-2026-10-18',
        product: 'L30',
        day: '2026-10-18',
        cloud: null,
        timeRange: null,
        availability: 'present',
        coverage: 'partial',
        granuleCount: 1,
      },
      splitPosition: 0.5,
    });

    render(<ImagerySplitDivider />);
    const slider = screen.getByRole('slider');

    fireEvent.keyDown(slider, { key: 'ArrowRight' });
    expect(useImageryStore.getState().splitPosition).toBeCloseTo(0.52);
    expect(mockGlobe.setRecentImagerySplitPosition).toHaveBeenCalled();

    fireEvent.keyDown(slider, { key: 'ArrowLeft' });
    expect(useImageryStore.getState().splitPosition).toBeCloseTo(0.5);

    fireEvent.keyDown(slider, { key: 'Home' });
    expect(useImageryStore.getState().splitPosition).toBe(0.05);

    fireEvent.keyDown(slider, { key: 'End' });
    expect(useImageryStore.getState().splitPosition).toBe(0.95);
  });
});
