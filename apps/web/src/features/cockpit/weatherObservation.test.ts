import { describe, expect, it, vi } from 'vitest';
import type { LavenderPlatform } from '../../platform';
import { fetchWeatherObservation } from './weatherObservation';

type ConditionsFn = NonNullable<LavenderPlatform['weatherConditions']>;

/** A platform whose optional weatherConditions is the only member the deck touches. */
function fakePlatform(impl: ConditionsFn | null): Pick<LavenderPlatform, 'weatherConditions'> {
  return impl == null ? {} : { weatherConditions: vi.fn(impl) };
}

describe('fetchWeatherObservation (platform adapter)', () => {
  it('maps the platform observation onto the deck provider and forwards the signal', async () => {
    const observation = { weatherCode: 95, cloudCoverPct: 90, precipitationMm: 12 };
    const platform = fakePlatform(async () => observation);
    const controller = new AbortController();
    const provider = fetchWeatherObservation(platform);

    await expect(provider(30, -97, controller.signal)).resolves.toEqual(observation);
    expect(platform.weatherConditions).toHaveBeenCalledWith(30, -97, {
      signal: controller.signal,
    });
  });

  it('resolves null when the platform has no observation (optional member absent)', async () => {
    const provider = fetchWeatherObservation(fakePlatform(null));
    await expect(provider(30, -97, new AbortController().signal)).resolves.toBe(null);
  });

  it('resolves null when the platform reports no observation', async () => {
    const provider = fetchWeatherObservation(fakePlatform(async () => null));
    await expect(provider(30, -97, new AbortController().signal)).resolves.toBe(null);
  });

  it('fails soft: upstream trouble resolves null instead of rejecting', async () => {
    const provider = fetchWeatherObservation(
      fakePlatform(async () => {
        throw new Error('network down');
      }),
    );
    await expect(provider(30, -97, new AbortController().signal)).resolves.toBe(null);
  });
});
