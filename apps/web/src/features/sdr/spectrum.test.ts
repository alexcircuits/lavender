import { describe, expect, it } from 'vitest';
import { analyzeSpectrum, findDirectionalFmPeak } from './spectrum';

function toneIq(frequencyOffset: number, sampleRate: number, samples = 2_048): ArrayBuffer {
  const iq = new Uint8Array(samples * 2);
  for (let index = 0; index < samples; index += 1) {
    const phase = (2 * Math.PI * frequencyOffset * index) / sampleRate;
    iq[index * 2] = Math.round(127.5 + 100 * Math.cos(phase));
    iq[index * 2 + 1] = Math.round(127.5 + 100 * Math.sin(phase));
  }
  return iq.buffer;
}

describe('spectrum', () => {
  it('FFT-shifted spectrum preserves the sign and offset of a complex tone', () => {
    const sampleRate = 2_048_000;
    const bins = analyzeSpectrum(toneIq(300_000, sampleRate));
    let peakIndex = 0;
    for (let index = 1; index < bins.length; index += 1) {
      if ((bins[index] ?? -Infinity) > (bins[peakIndex] ?? -Infinity)) {
        peakIndex = index;
      }
    }
    const offset = (peakIndex - bins.length / 2) * (sampleRate / bins.length);
    expect(Math.abs(offset - 300_000)).toBeLessThan(2_000);
  });

  it('directional FM seek chooses the nearest rounded station in-band', () => {
    const sampleRate = 2_048_000;
    const bins = analyzeSpectrum(toneIq(300_000, sampleRate));
    const forward = findDirectionalFmPeak(bins, 98_500_000, sampleRate, 1, 98_500_000);
    expect(forward).not.toBeNull();
    expect(forward?.frequency).toBe(98_800_000);
    expect(forward?.snr).toBeGreaterThan(6);

    const reverse = findDirectionalFmPeak(bins, 98_500_000, sampleRate, -1, 98_500_000);
    expect(reverse).toBeNull();
  });

  it('directional FM seek passes over the skipped channel to the next station', () => {
    const sampleRate = 2_048_000;
    const bins = new Float32Array(2_048).fill(-80);
    // Carriers at 98.8 and 99.1 MHz in a frame centred on 98.5 MHz.
    for (const offset of [300_000, 600_000]) {
      const index = Math.round(offset / (sampleRate / bins.length) + bins.length / 2);
      for (let bin = index - 5; bin <= index + 5; bin += 1) {
        bins[bin] = -10;
      }
    }

    const seek = (skipFrequency?: number) =>
      findDirectionalFmPeak(bins, 98_500_000, sampleRate, 1, 98_500_000, {
        skipFrequency,
      })?.frequency;

    expect(seek(undefined)).toBe(98_800_000);
    expect(seek(98_800_000)).toBe(99_100_000);
    // The skip covers channels up to and including 75 kHz away.
    expect(seek(98_725_000)).toBe(99_100_000);
    expect(seek(98_724_999)).toBe(98_800_000);
  });

  it('handles empty or small buffers gracefully', () => {
    expect(analyzeSpectrum(new Uint8Array(0))).toHaveLength(0);
    expect(analyzeSpectrum(new Uint8Array(2))).toHaveLength(0);
    expect(
      findDirectionalFmPeak(new Float32Array(8), 98_500_000, 2_048_000, 1, 98_500_000),
    ).toBeNull();
  });
});
