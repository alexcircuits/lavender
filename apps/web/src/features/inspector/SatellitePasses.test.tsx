import { act, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { renderWithRuntime } from '../../app/testing';
import type { SatellitePasses as PassResult } from '../../contracts';
import { DesignProvider } from '../../design';
import { createRuntime, RuntimeProvider } from '../../runtime';
import { catalogOf, FakePlatform, fakeGlobe, layer } from '../../test/fakes';
import { SatellitePasses } from './SatellitePasses';

const RESULT: PassResult = {
  noradId: 25544,
  name: 'ISS (ZARYA)',
  observer: { lat: 60.17, lon: 24.94, altM: 0 },
  from: '2026-10-13T23:00:00Z',
  until: '2026-10-14T23:00:00Z',
  minElevationDeg: 0,
  passes: [
    {
      rise: '2026-10-13T23:59:00Z',
      peak: '2026-10-14T00:03:00Z',
      set: '2026-10-14T00:07:00Z',
      maxElevationDeg: 56.25,
      alreadyAbove: false,
      continuesAfterWindow: false,
    },
    {
      rise: '2026-10-13T23:00:00Z',
      peak: '2026-10-14T23:00:00Z',
      set: '2026-10-14T23:00:00Z',
      maxElevationDeg: 23.7,
      alreadyAbove: true,
      continuesAfterWindow: true,
    },
  ],
};

function deferred<T>() {
  let resolve: ((value: T) => void) | undefined;
  let reject: ((error: unknown) => void) | undefined;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return {
    promise,
    resolve: (value: T) => resolve?.(value),
    reject: (error: unknown) => reject?.(error),
  };
}

const runtimes: ReturnType<typeof createRuntime>[] = [];
function setup(noradId = 25544, withGlobe = true) {
  const platform = new FakePlatform();
  const runtime = createRuntime(platform, { workspaceLocation: null });
  runtimes.push(runtime);
  runtime.stores.system.getState().setCatalog(catalogOf([layer('satellites')]));
  const fake = fakeGlobe();
  vi.mocked(fake.globe.cameraPose).mockReturnValue({
    lon: 24.94,
    lat: 60.17,
    heightM: 20_000_000,
    headingRad: 0,
    pitchRad: -Math.PI / 2,
    rollRad: 0,
  });
  vi.mocked(fake.globe.centrePosition).mockReturnValue({ lat: 60.17, lon: 24.94 });
  if (withGlobe) runtime.attachGlobe(fake.globe);
  const rendered = renderWithRuntime(runtime, <SatellitePasses noradId={noradId} />);
  return { platform, runtime, fake, rendered, user: userEvent.setup() };
}

afterEach(() => {
  for (const runtime of runtimes.splice(0)) act(() => runtime.detachGlobe());
  vi.restoreAllMocks();
});

describe('satellite pass predictions', () => {
  it('requests passes only on demand with the ground-level observer, without moving the camera', async () => {
    const { platform, fake, user } = setup();
    platform.satellitePasses.mockResolvedValue(RESULT);
    expect(platform.satellitePasses).not.toHaveBeenCalled();
    await user.click(screen.getByRole('button', { name: 'Predict passes at map centre' }));
    expect(platform.satellitePasses).toHaveBeenCalledExactlyOnceWith(
      {
        noradId: 25544,
        observer: { lat: 60.17, lon: 24.94, altM: 0 },
        hours: 24,
        minElevationDeg: 0,
      },
      expect.objectContaining({ signal: expect.any(AbortSignal) }),
    );
    expect(await screen.findByText('2 predicted passes')).toBeVisible();
    expect(fake.globe.flyTo).not.toHaveBeenCalled();
    expect(fake.globe.restoreCamera).not.toHaveBeenCalled();
    expect(fake.globe.home).not.toHaveBeenCalled();
    expect(screen.getByText('Observer: 60.1700° N, 24.9400° E · 0 m altitude')).toBeVisible();
    expect(
      screen.getByText('Window: 2026-10-13 23:00:00 UTC – 2026-10-14 23:00:00 UTC'),
    ).toBeVisible();
    expect(screen.getByText(/Geometric passes at or above 0° elevation/)).toHaveTextContent(
      'Sunlight, atmospheric refraction and naked-eye visibility are not modelled.',
    );
    const table = await screen.findByRole('table', { name: 'Satellite passes' });
    const crossing = within(table).getAllByRole('row')[1];
    expect(crossing).toHaveTextContent('2026-10-13 23:59:00 UTC');
    expect(crossing).toHaveTextContent('2026-10-14 00:03:00 UTC');
    expect(crossing).toHaveTextContent('2026-10-14 00:07:00 UTC');
    expect(crossing).toHaveTextContent('56.3°');
    expect(crossing).toHaveTextContent('Rises in window');
    expect(crossing).toHaveTextContent('Sets in window');
    const clipped = within(table).getAllByRole('row')[2];
    expect(clipped).toHaveTextContent('Ongoing at window start');
    expect(clipped).toHaveTextContent('Clipped at window end');
  });

  it('shows an honest empty prediction result with the observer and time window', async () => {
    const { platform, user } = setup();
    platform.satellitePasses.mockResolvedValue({ ...RESULT, passes: [] });
    await user.click(screen.getByRole('button', { name: 'Predict passes at map centre' }));
    expect(await screen.findByText('0 predicted passes')).toBeVisible();
    expect(
      screen.getByText('No geometric passes reach this elevation in the requested window.'),
    ).toBeVisible();
    expect(screen.queryByRole('table', { name: 'Satellite passes' })).not.toBeInTheDocument();
  });

  it('disables prediction while pending, reports an error and allows retry', async () => {
    const { platform, user } = setup();
    const first = deferred<PassResult>();
    platform.satellitePasses.mockReturnValueOnce(first.promise);
    await user.click(screen.getByRole('button', { name: 'Predict passes at map centre' }));
    expect(screen.getByRole('button', { name: 'Predicting passes…' })).toBeDisabled();
    await user.click(screen.getByRole('button', { name: 'Predicting passes…' }));
    expect(platform.satellitePasses).toHaveBeenCalledTimes(1);
    await act(async () =>
      first.reject({
        code: 'unavailable',
        message: 'Orbit elements are unavailable.',
        retryable: true,
      }),
    );
    expect(
      within(screen.getByRole('region', { name: 'Satellite pass predictions' })).getByRole(
        'status',
      ),
    ).toHaveTextContent('Orbit elements are unavailable.');
    platform.satellitePasses.mockResolvedValueOnce({ ...RESULT, passes: [] });
    await user.click(screen.getByRole('button', { name: 'Predict passes at map centre' }));
    expect(await screen.findByText('0 predicted passes')).toBeVisible();
    expect(platform.satellitePasses).toHaveBeenCalledTimes(2);
  });

  it('aborts a previous satellite request and ignores its late settlement after the selection changes', async () => {
    const { platform, runtime, rendered, user } = setup();
    const previous = deferred<PassResult>();
    platform.satellitePasses.mockReturnValueOnce(previous.promise);
    await user.click(screen.getByRole('button', { name: 'Predict passes at map centre' }));
    const signal = platform.satellitePasses.mock.calls[0]?.[1]?.signal;
    rendered.rerender(
      <RuntimeProvider runtime={runtime}>
        <DesignProvider>
          <SatellitePasses noradId={20580} />
        </DesignProvider>
      </RuntimeProvider>,
    );
    expect(signal?.aborted).toBe(true);
    await act(async () => previous.resolve(RESULT));
    expect(screen.queryByText('2 predicted passes')).not.toBeInTheDocument();
    platform.satellitePasses.mockResolvedValueOnce({ ...RESULT, noradId: 20580, passes: [] });
    await user.click(screen.getByRole('button', { name: 'Predict passes at map centre' }));
    expect(await screen.findByText('0 predicted passes')).toBeVisible();
    expect(platform.satellitePasses.mock.calls.at(-1)?.[0].noradId).toBe(20580);
  });

  it('aborts requests on unmount and ignores late errors', async () => {
    const { platform, rendered, user } = setup();
    const pending = deferred<PassResult>();
    platform.satellitePasses.mockReturnValueOnce(pending.promise);
    await user.click(screen.getByRole('button', { name: 'Predict passes at map centre' }));
    const signal = platform.satellitePasses.mock.calls[0]?.[1]?.signal;
    rendered.unmount();
    expect(signal?.aborted).toBe(true);
    await act(async () => pending.reject(new Error('late response')));
    expect(screen.queryByText('late response')).not.toBeInTheDocument();
  });

  it('waits for a mounted globe and does not send invalid observer coordinates', async () => {
    const { platform, runtime, fake, user } = setup(25544, false);
    expect(screen.getByRole('button', { name: 'Predict passes at map centre' })).toBeDisabled();
    vi.mocked(fake.globe.centrePosition).mockReturnValue(null);
    act(() => runtime.attachGlobe(fake.globe));
    await user.click(screen.getByRole('button', { name: 'Predict passes at map centre' }));
    expect(
      within(screen.getByRole('region', { name: 'Satellite pass predictions' })).getByRole(
        'status',
      ),
    ).toHaveTextContent('Point the map centre at the ground before choosing an observer.');
    expect(platform.satellitePasses).not.toHaveBeenCalled();
  });

  it('uses the visible ground centre when a tilted camera is above a different point', async () => {
    const { platform, fake, user } = setup();
    vi.mocked(fake.globe.cameraPose).mockReturnValue({
      lon: 10,
      lat: 20,
      heightM: 400_000,
      headingRad: 0,
      pitchRad: -0.4,
      rollRad: 0,
    });
    vi.mocked(fake.globe.centrePosition).mockReturnValue({ lat: 30, lon: 40, altM: 1500 });
    await user.click(screen.getByRole('button', { name: 'Predict passes at map centre' }));
    expect(platform.satellitePasses.mock.calls[0]?.[0].observer).toEqual({
      lat: 30,
      lon: 40,
      altM: 0,
    });
  });
});
