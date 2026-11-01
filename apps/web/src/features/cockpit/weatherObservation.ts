/**
 * Cockpit weather observation source (LB-161): the client-side port of upstream's
 * regional weather provider (`legacy/server/providers/regional/weather.js` +
 * `normalizeRegionalWeather`) — Open-Meteo current conditions mapped onto the
 * `WeatherEffectInput` the WASM weather kernel consumes. Upstream proxied this
 * through its server; Lavender reaches it behind the platform boundary
 * (`platform.weatherConditions`, ADR 0005: feature code never fetches) and fails
 * soft — a null observation yields a zeroed profile and the deck draws nothing.
 */

import type { WeatherEffectInput } from '../../globe/renderers/wasmSkyKinematics';
import type { LavenderPlatform } from '../../platform';

export type WeatherObservationProvider = (
  latitude: number,
  longitude: number,
  signal: AbortSignal,
) => Promise<WeatherEffectInput | null>;

/**
 * Adapts the platform's optional weather observation into the deck's provider.
 * Upstream trouble is not an error: rejections resolve null so the deck keeps its
 * last profile and eases out on the following strength steps (upstream fails soft).
 */
export function fetchWeatherObservation(
  platform: Pick<LavenderPlatform, 'weatherConditions'>,
): WeatherObservationProvider {
  return async (latitude, longitude, signal) => {
    try {
      return (await platform.weatherConditions?.(latitude, longitude, { signal })) ?? null;
    } catch {
      return null;
    }
  };
}
