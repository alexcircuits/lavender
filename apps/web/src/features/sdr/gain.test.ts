import { describe, expect, it } from 'vitest';
import {
  normalizeSdrGain,
  R820T_GAIN_STEPS_DB,
  readSdrGainSettings,
  SDR_GAIN_AUTO,
  SDR_GAIN_DEFAULTS,
  SDR_GAIN_STORAGE_KEY,
  tunerGainValue,
  writeSdrGainSettings,
} from './gain';
import type { SdrStorageLike } from './usbDevices';

function memoryStorage(initial: Record<string, string> = {}): SdrStorageLike & {
  values: Map<string, string>;
} {
  const values = new Map<string, string>(Object.entries(initial));
  return {
    values,
    getItem: (key: string) => (values.has(key) ? (values.get(key) ?? null) : null),
    setItem: (key: string, value: string) => values.set(key, String(value)),
  };
}

describe('gain', () => {
  it('gain steps are the R820T table and ADS-B defaults to manual 28.0 dB', () => {
    expect(R820T_GAIN_STEPS_DB).toHaveLength(29);
    expect(R820T_GAIN_STEPS_DB[0]).toBe(0.0);
    expect(R820T_GAIN_STEPS_DB.at(-1)).toBe(49.6);
    expect(R820T_GAIN_STEPS_DB).toContain(20.7);
    expect(R820T_GAIN_STEPS_DB).toContain(28.0);
    expect(SDR_GAIN_DEFAULTS).toEqual({ fm: 'auto', adsb: 28.0 });
    expect(tunerGainValue(SDR_GAIN_AUTO)).toBeNull();
    expect(tunerGainValue(20.7)).toBe(20.7);
  });

  it('requested gains normalize to AUTO or the nearest listed step', () => {
    expect(normalizeSdrGain('AUTO')).toBe('auto');
    expect(normalizeSdrGain('36.4')).toBe(36.4);
    expect(normalizeSdrGain(21)).toBe(20.7);
    expect(normalizeSdrGain(100)).toBe(49.6);
    expect(normalizeSdrGain(-3)).toBe(0.0);
    expect(normalizeSdrGain('loud')).toBeNull();
    expect(normalizeSdrGain(null)).toBeNull();
    expect(normalizeSdrGain(undefined)).toBeNull();
    expect(normalizeSdrGain('')).toBeNull();
  });

  it('per-mode gain persists in storage and survives unavailable storage', () => {
    const storage = memoryStorage();
    expect(readSdrGainSettings(storage)).toEqual(SDR_GAIN_DEFAULTS);
    expect(writeSdrGainSettings(storage, { fm: 28, adsb: 'auto' })).toBe(true);
    expect(storage.values.get(SDR_GAIN_STORAGE_KEY)).toBe('{"fm":28,"adsb":"auto"}');
    expect(readSdrGainSettings(storage)).toEqual({ fm: 28, adsb: 'auto' });

    const corrupt = memoryStorage({ [SDR_GAIN_STORAGE_KEY]: '{not json' });
    expect(readSdrGainSettings(corrupt)).toEqual(SDR_GAIN_DEFAULTS);

    const throwing: SdrStorageLike = {
      getItem() {
        throw new Error('blocked');
      },
      setItem() {
        throw new Error('blocked');
      },
    };
    expect(readSdrGainSettings(throwing)).toEqual(SDR_GAIN_DEFAULTS);
    expect(writeSdrGainSettings(throwing, SDR_GAIN_DEFAULTS)).toBe(false);
    expect(readSdrGainSettings(null)).toEqual(SDR_GAIN_DEFAULTS);
  });

  it('a stored ADS-B gain wins over the 28.0 dB default', () => {
    const storage = memoryStorage({
      [SDR_GAIN_STORAGE_KEY]: JSON.stringify({ fm: 'auto', adsb: 20.7 }),
    });
    expect(readSdrGainSettings(storage)).toEqual({ fm: 'auto', adsb: 20.7 });

    const onlyFm = memoryStorage({
      [SDR_GAIN_STORAGE_KEY]: JSON.stringify({ fm: 36.4 }),
    });
    expect(readSdrGainSettings(onlyFm)).toEqual({ fm: 36.4, adsb: 28.0 });
  });
});
