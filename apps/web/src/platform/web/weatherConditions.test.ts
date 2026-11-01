import { describe, expect, it } from 'vitest';
import { fetchWeatherConditions, normalizeWeatherObservation } from './weatherConditions';

const openMeteoBody = JSON.stringify({
  current: {
    weather_code: 95,
    cloud_cover: 90,
    precipitation: 12,
    wind_speed_10m: 60,
    wind_direction_10m: 90,
    visibility: 8000,
  },
});

const okResponse = (body: string): Response =>
  new Response(body, { status: 200, headers: { 'Content-Type': 'application/json' } });

describe('normalizeWeatherObservation', () => {
  it('normalizes an Open-Meteo current payload', () => {
    expect(
      normalizeWeatherObservation({
        current: {
          weather_code: 61,
          cloud_cover: 84.4,
          precipitation: 2.5,
          wind_speed_10m: 31,
          wind_direction_10m: 210,
          visibility: 12000,
        },
      }),
    ).toEqual({
      weatherCode: 61,
      cloudCoverPct: 84.4,
      precipitationMm: 2.5,
      windKph: 31,
      windDirectionDeg: 210,
      visibilityM: 12000,
    });
  });

  it('returns null without a usable weather code and tolerates missing fields', () => {
    expect(normalizeWeatherObservation(null)).toBe(null);
    expect(normalizeWeatherObservation({})).toBe(null);
    expect(normalizeWeatherObservation({ current: { weather_code: 'clear' } })).toBe(null);
    expect(normalizeWeatherObservation({ current: { weather_code: 45 } })).toEqual({
      weatherCode: 45,
    });
  });
});

describe('fetchWeatherConditions', () => {
  it('calls Open-Meteo current conditions and normalizes the body', async () => {
    const calls: Array<{ input: string; init?: RequestInit | undefined }> = [];
    const fetchLike = async (input: string, init?: RequestInit) => {
      calls.push(init === undefined ? { input } : { input, init });
      return okResponse(openMeteoBody);
    };
    const controller = new AbortController();
    await expect(
      fetchWeatherConditions(fetchLike, 30.123456, -97.654321, { signal: controller.signal }),
    ).resolves.toEqual({
      weatherCode: 95,
      cloudCoverPct: 90,
      precipitationMm: 12,
      windKph: 60,
      windDirectionDeg: 90,
      visibilityM: 8000,
    });
    expect(calls[0]?.input.startsWith('https://api.open-meteo.com/v1/forecast?')).toBe(true);
    const params = new URL(calls[0]?.input ?? '').searchParams;
    expect(params.get('latitude')).toBe('30.12346');
    expect(params.get('longitude')).toBe('-97.65432');
    expect(params.get('current')).toContain('weather_code');
    expect(calls[0]?.init?.signal).toBe(controller.signal);
  });

  it('fails soft on transport errors, non-200s, oversized bodies and bad JSON', async () => {
    const fail = async () => {
      throw new Error('offline');
    };
    expect(await fetchWeatherConditions(fail, 30, -97)).toBe(null);
    expect(
      await fetchWeatherConditions(async () => new Response('nope', { status: 502 }), 30, -97),
    ).toBe(null);
    expect(
      await fetchWeatherConditions(async () => okResponse('x'.repeat(512 * 1024 + 1)), 30, -97),
    ).toBe(null);
    expect(await fetchWeatherConditions(async () => okResponse('<html>'), 30, -97)).toBe(null);
  });
});
