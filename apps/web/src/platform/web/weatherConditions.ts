/**
 * Cockpit weather observation source (LB-161): the client-side port of upstream's
 * regional weather provider (`legacy/server/providers/regional/weather.js` +
 * `normalizeRegionalWeather`). Upstream proxied Open-Meteo through its server;
 * Lavender's backend has no weather route, so the web platform calls the public
 * endpoint directly and fails soft (null observation → zeroed profile → the
 * cockpit cloud deck draws nothing). Living behind the platform boundary keeps
 * feature code fetch-free (ADR 0005).
 */
import type { WeatherObservation } from '../types';

const OPEN_METEO_FORECAST_URL = 'https://api.open-meteo.com/v1/forecast';

/** Largest accepted response body; the payload is one small JSON object. */
const MAX_RESPONSE_BYTES = 512 * 1024;

function numberOrNull(value: unknown): number | null {
  const n = Number(value);
  return Number.isFinite(n) ? n : null;
}

/**
 * Normalizes an Open-Meteo `current` payload into a `WeatherObservation`.
 * Exported for tests; returns null for payloads without usable conditions.
 */
export function normalizeWeatherObservation(payload: unknown): WeatherObservation | null {
  const current = (payload as { current?: Record<string, unknown> } | null)?.current;
  if (!current || numberOrNull(current.weather_code) == null) return null;
  return {
    weatherCode: numberOrNull(current.weather_code) ?? undefined,
    cloudCoverPct: numberOrNull(current.cloud_cover) ?? undefined,
    precipitationMm: numberOrNull(current.precipitation) ?? undefined,
    windKph: numberOrNull(current.wind_speed_10m) ?? undefined,
    windDirectionDeg: numberOrNull(current.wind_direction_10m) ?? undefined,
    visibilityM: numberOrNull(current.visibility) ?? undefined,
  };
}

/** Fetches current conditions at a point for the cockpit cloud deck. */
export async function fetchWeatherConditions(
  fetchLike: (input: string, init?: RequestInit) => Promise<Response>,
  latitude: number,
  longitude: number,
  options?: { readonly signal?: AbortSignal },
): Promise<WeatherObservation | null> {
  const params = new URLSearchParams({
    latitude: latitude.toFixed(5),
    longitude: longitude.toFixed(5),
    current: 'precipitation,weather_code,cloud_cover,wind_speed_10m,wind_direction_10m,visibility',
    timezone: 'UTC',
  });
  let response: Response;
  try {
    response = await fetchLike(`${OPEN_METEO_FORECAST_URL}?${params}`, {
      ...(options?.signal ? { signal: options.signal } : {}),
    });
  } catch {
    return null;
  }
  if (!response.ok) return null;
  const body = await response.text();
  if (body.length > MAX_RESPONSE_BYTES) return null;
  try {
    return normalizeWeatherObservation(JSON.parse(body) as unknown);
  } catch {
    return null;
  }
}
