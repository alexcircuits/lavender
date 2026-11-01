import { act, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { installRasterCanvas, type RasterContext } from '../../test/rasterCanvas';
import { CockpitCloudDeck } from './CockpitCloudDeck';
import { CLOUD_MAX_FPS, CLOUD_MIN_FPS, cloudFrameIntervalMs } from './cloudDeck';
import type { WeatherObservationProvider } from './weatherObservation';

vi.mock('../../runtime', () => ({
  usePlatform: vi.fn(() => ({})),
}));

const ANCHOR = { latitude: 30, longitude: -97, altitudeM: 2000 };

const stormObservation = {
  weatherCode: 95,
  cloudCoverPct: 90,
  precipitationMm: 12,
  windKph: 60,
  windDirectionDeg: 90,
  visibilityM: 8000,
};

describe('CockpitCloudDeck', () => {
  let raster: ReturnType<typeof installRasterCanvas>;
  let context: RasterContext;

  beforeEach(() => {
    vi.useFakeTimers();
    raster = installRasterCanvas();
  });

  afterEach(() => {
    vi.useRealTimers();
    raster.restore();
    vi.restoreAllMocks();
  });

  const mountDeck = (provider: WeatherObservationProvider) => {
    const view = render(<CockpitCloudDeck anchor={ANCHOR} weatherProvider={provider} />);
    const canvas = view.container.querySelector('canvas');
    if (!(canvas instanceof HTMLCanvasElement)) throw new Error('deck canvas missing');
    context = raster.contextOf(canvas);
    return { view, canvas };
  };

  it('fetches the anchor observation and applies the kernel profile', async () => {
    const provider = vi.fn(async () => stormObservation);
    const { canvas } = mountDeck(provider);

    // The 1 Hz refresh check fires, fetches and applies: the eased strength
    // crosses the visibility threshold and the deck activates.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1500);
    });
    expect(provider).toHaveBeenCalledWith(
      ANCHOR.latitude,
      ANCHOR.longitude,
      expect.any(AbortSignal),
    );
    expect(canvas.dataset.weatherCode).toBe('95');
    expect(canvas.dataset.active).toBe('true');
    // The kernel's frame-cost hint set the interval: storm ≈ 74 ms (12–15 fps window).
    const stormCost = 0.22 * 0.92 + 0.2 * 0.68 + 0.24 * 0.55 + 0.08 * (0.68 * (0.58 + 0.55 * 0.42));
    expect(cloudFrameIntervalMs(stormCost)).toBeGreaterThan(1000 / CLOUD_MAX_FPS);
    expect(cloudFrameIntervalMs(stormCost)).toBeLessThan(1000 / CLOUD_MIN_FPS);
  });

  it('renders on the frame-budget clamp, not every rAF tick', async () => {
    mountDeck(vi.fn(async () => stormObservation));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1500);
    });
    const clearRect = vi.spyOn(context, 'clearRect');
    clearRect.mockClear();

    // Half of the storm frame budget (~74 ms): no repaint yet.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(30);
    });
    expect(clearRect).not.toHaveBeenCalled();

    // Past the budget: exactly one repaint lands on the next tick.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(80);
    });
    expect(clearRect.mock.calls.length).toBeGreaterThanOrEqual(1);
    expect(clearRect.mock.calls.length).toBeLessThanOrEqual(2);
  });

  it('keeps the deck clear when the observation is unavailable', async () => {
    const provider = vi.fn(async () => null);
    const { canvas } = mountDeck(provider);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1500);
    });
    expect(provider).toHaveBeenCalled();
    expect(canvas.dataset.weatherCode).toBeUndefined();
    expect(canvas.dataset.active).toBe('false');
    const clearRect = vi.spyOn(context, 'clearRect');
    await act(async () => {
      await vi.advanceTimersByTimeAsync(300);
    });
    expect(clearRect).not.toHaveBeenCalled();
  });

  it('stops the loop and aborts the observation on unmount', async () => {
    const provider = vi.fn(
      (_lat: number, _lon: number, signal: AbortSignal) =>
        new Promise<null>((resolve) => {
          signal.addEventListener('abort', () => resolve(null));
        }),
    );
    const { view } = mountDeck(provider);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1100);
    });
    expect(provider).toHaveBeenCalledTimes(1);
    view.unmount();
    const signal = provider.mock.calls[0]?.[2];
    expect(signal?.aborted).toBe(true);
    const calls = provider.mock.calls.length;
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5000);
    });
    expect(provider).toHaveBeenCalledTimes(calls);
  });
});
