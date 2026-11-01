/**
 * Tuner gain settings for the local RTL-SDR, remembered per receiver mode.
 *
 * `'auto'` hands gain to the tuner's AGC. Manual values are the R820T gain
 * steps librtlsdr and dump1090 expose, in dB. Receivers with an onboard LNA
 * (for example a Nooelec FlyCatcher with its LNA on) overload at AGC or high
 * gain, so the ADS-B default is a fixed step. It is 28.0 dB rather than the
 * lower steps dump1090 users pick because the WebUSB library maps dB to tuner
 * gain stages differently from librtlsdr: on a FlyCatcher, 20.7 dB here gave
 * about 1 msg/s and 28.0 dB about 9 msg/s on the same antenna.
 */

import type { SdrStorageLike } from './usbDevices';

export const SDR_GAIN_AUTO = 'auto' as const;

export type SdrGainValue = typeof SDR_GAIN_AUTO | number;

/** R820T tuner gain steps in dB, as listed by librtlsdr. */
export const R820T_GAIN_STEPS_DB: readonly number[] = Object.freeze([
  0.0, 0.9, 1.4, 2.7, 3.7, 7.7, 8.7, 12.5, 14.4, 15.7, 16.6, 19.7, 20.7, 22.9, 25.4, 28.0, 29.7,
  32.8, 33.8, 36.4, 37.2, 38.6, 40.2, 42.1, 43.4, 43.9, 44.5, 48.0, 49.6,
]);

export interface SdrGainSettings {
  readonly fm: SdrGainValue;
  readonly adsb: SdrGainValue;
}

export const SDR_GAIN_DEFAULTS: SdrGainSettings = Object.freeze({
  fm: SDR_GAIN_AUTO,
  adsb: 28.0,
});

export const SDR_GAIN_STORAGE_KEY = 'gev:sdr:gain:v1';

/**
 * Normalize a requested gain to `'auto'` or the nearest listed R820T step.
 */
export function normalizeSdrGain(value: unknown): SdrGainValue | null {
  if (value === null || value === undefined || value === '') return null;
  if (typeof value === 'string' && value.trim().toLowerCase() === SDR_GAIN_AUTO) {
    return SDR_GAIN_AUTO;
  }
  const requested = typeof value === 'number' ? value : Number(value);
  if (!Number.isFinite(requested)) return null;
  let nearest = R820T_GAIN_STEPS_DB[0] ?? 0.0;
  for (const step of R820T_GAIN_STEPS_DB) {
    if (Math.abs(step - requested) < Math.abs(nearest - requested)) {
      nearest = step;
    }
  }
  return nearest;
}

/**
 * Read per-mode gain settings, falling back to the defaults.
 */
export function readSdrGainSettings(storage?: SdrStorageLike | Storage | null): SdrGainSettings {
  let stored: Record<string, unknown> | null = null;
  try {
    const raw = storage?.getItem?.(SDR_GAIN_STORAGE_KEY);
    stored = raw ? JSON.parse(raw) : null;
  } catch {
    stored = null;
  }
  return {
    fm: normalizeSdrGain(stored?.fm) ?? SDR_GAIN_DEFAULTS.fm,
    adsb: normalizeSdrGain(stored?.adsb) ?? SDR_GAIN_DEFAULTS.adsb,
  };
}

/**
 * Persist per-mode gain settings. Storage failures are ignored.
 */
export function writeSdrGainSettings(
  storage: SdrStorageLike | Storage | null | undefined,
  settings: SdrGainSettings,
): boolean {
  try {
    storage?.setItem?.(
      SDR_GAIN_STORAGE_KEY,
      JSON.stringify({ fm: settings.fm, adsb: settings.adsb }),
    );
    return Boolean(storage?.setItem);
  } catch {
    return false;
  }
}

/**
 * The value handed to the tuner: null selects AGC.
 */
export function tunerGainValue(setting: SdrGainValue): number | null {
  return setting === SDR_GAIN_AUTO ? null : setting;
}
