import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { RecentImageryResponse } from '../../platform/recentImageryTypes';
import { usePlatform, useUi } from '../../runtime';
import { useImageryStore } from './imageryStore';
import { RecentImageryPanel } from './RecentImageryPanel';

vi.mock('../../runtime', () => ({
  usePlatform: vi.fn(),
  useUi: vi.fn(),
  useSystem: vi.fn(),
}));

describe('RecentImageryPanel', () => {
  let mockPlatform: {
    recentImagery: ReturnType<typeof vi.fn>;
  };
  let mockGlobe: {
    setRecentImageryBox: ReturnType<typeof vi.fn>;
    setRecentImagerySlot: ReturnType<typeof vi.fn>;
    setRecentImageryMode: ReturnType<typeof vi.fn>;
    setRecentImageryOpacity: ReturnType<typeof vi.fn>;
    startImageryBoxDrawing: ReturnType<typeof vi.fn>;
    cancelImageryBoxDrawing: ReturnType<typeof vi.fn>;
    centrePosition: ReturnType<typeof vi.fn>;
  };

  const mockResponse: RecentImageryResponse = {
    candidates: [
      {
        key: 'S30-2026-10-20',
        product: 'S30',
        day: '2026-10-20',
        cloud: { min: 8, max: 12 },
        timeRange: null,
        availability: 'present',
        coverage: 'full',
        granuleCount: 2,
      },
      {
        key: 'L30-2026-10-18',
        product: 'L30',
        day: '2026-10-18',
        cloud: { min: 25, max: 35 },
        timeRange: null,
        availability: 'present',
        coverage: 'partial',
        granuleCount: 1,
      },
    ],
    rankedLatest: {
      candidate: {
        key: 'S30-2026-10-20',
        product: 'S30',
        day: '2026-10-20',
        cloud: { min: 8, max: 12 },
        timeRange: null,
        availability: 'present',
        coverage: 'full',
        granuleCount: 2,
      },
      reason: 'clear',
      certain: true,
    },
    tileTemplates: {
      S30: 'https://gibs.earthdata.nasa.gov/s30',
      L30: 'https://gibs.earthdata.nasa.gov/l30',
    },
    attribution: {
      credit: 'NASA GIBS',
      licence: 'US Gov',
      statement: 'We acknowledge the use of imagery from NASA GIBS.',
    },
    truncated: false,
    errors: [],
  };

  beforeEach(() => {
    useImageryStore.getState().clear();

    mockPlatform = {
      recentImagery: vi.fn().mockResolvedValue(mockResponse),
    };
    (usePlatform as unknown as ReturnType<typeof vi.fn>).mockReturnValue(mockPlatform);

    mockGlobe = {
      setRecentImageryBox: vi.fn(),
      setRecentImagerySlot: vi.fn(),
      setRecentImageryMode: vi.fn(),
      setRecentImageryOpacity: vi.fn(),
      startImageryBoxDrawing: vi.fn(),
      cancelImageryBoxDrawing: vi.fn(),
      centrePosition: vi.fn().mockReturnValue({ lon: -97.74, lat: 30.26 }),
    };
    (useUi as unknown as ReturnType<typeof vi.fn>).mockImplementation((selector) =>
      selector({ globe: mockGlobe }),
    );
  });

  it('renders initial empty state with draw and current view buttons', () => {
    render(<RecentImageryPanel />);
    expect(screen.getByText('Recent satellite imagery')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Draw Box/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Current View/i })).toBeInTheDocument();
    expect(
      screen.getByText(/Draw an area or use the current view to search recent satellite passes/i),
    ).toBeInTheDocument();
  });

  it('triggers search when Current View is clicked and selects recommended pass', async () => {
    render(<RecentImageryPanel />);
    const currentViewBtn = screen.getByRole('button', { name: /Current View/i });
    fireEvent.click(currentViewBtn);

    expect(mockPlatform.recentImagery).toHaveBeenCalled();
    await waitFor(() => {
      expect(screen.getByText('Available Passes (2)')).toBeInTheDocument();
    });

    expect(mockGlobe.setRecentImageryBox).toHaveBeenCalled();
    expect(mockGlobe.setRecentImagerySlot).toHaveBeenCalledWith('a', expect.any(Object));
    expect(
      screen.getByText('We acknowledge the use of imagery from NASA GIBS.'),
    ).toBeInTheDocument();
  });

  it('allows toggling comparison modes and adjusting layer opacity', async () => {
    render(<RecentImageryPanel />);
    fireEvent.click(screen.getByRole('button', { name: /Current View/i }));

    await waitFor(() => {
      expect(screen.getByText('vs Basemap')).toBeInTheDocument();
    });

    // Switch to vs Basemap
    fireEvent.click(screen.getByText('vs Basemap'));
    expect(mockGlobe.setRecentImageryMode).toHaveBeenCalledWith('basemap');
    expect(useImageryStore.getState().mode).toBe('basemap');

    // Switch to A/B Swipe
    fireEvent.click(screen.getByText('A / B Swipe'));
    expect(mockGlobe.setRecentImageryMode).toHaveBeenCalledWith('ab');
    expect(useImageryStore.getState().mode).toBe('ab');

    // Adjust opacity
    const slider = screen.getByLabelText('Layer opacity');
    fireEvent.change(slider, { target: { value: '0.75' } });
    expect(mockGlobe.setRecentImageryOpacity).toHaveBeenCalledWith(0.75);
    expect(useImageryStore.getState().opacity).toBe(0.75);
  });

  it('toggles interactive box drawing session on globe', () => {
    render(<RecentImageryPanel />);
    const drawBtn = screen.getByRole('button', { name: /Draw Box/i });
    fireEvent.click(drawBtn);

    expect(mockGlobe.startImageryBoxDrawing).toHaveBeenCalled();
    expect(useImageryStore.getState().isDrawingBox).toBe(true);

    // Cancel drawing
    const cancelBtn = screen.getByRole('button', { name: /Cancel Draw/i });
    fireEvent.click(cancelBtn);
    expect(mockGlobe.cancelImageryBoxDrawing).toHaveBeenCalled();
    expect(useImageryStore.getState().isDrawingBox).toBe(false);
  });

  it('clears selection and layers on Clear button click', async () => {
    render(<RecentImageryPanel />);
    fireEvent.click(screen.getByRole('button', { name: /Current View/i }));

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Clear/i })).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole('button', { name: /Clear/i }));
    expect(mockGlobe.setRecentImageryBox).toHaveBeenCalledWith(null);
    expect(mockGlobe.setRecentImagerySlot).toHaveBeenCalledWith('a', null);
    expect(useImageryStore.getState().box).toBeNull();
  });
});
