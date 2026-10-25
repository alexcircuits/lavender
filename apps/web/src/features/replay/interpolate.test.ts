import { describe, expect, it } from 'vitest';
import { calculateBearing, interpolateSample } from './interpolate';
import type { ReplaySample } from './types';

describe('calculateBearing', () => {
  it('calculates cardinal bearings correctly', () => {
    // Due North
    expect(calculateBearing(0, 0, 10, 0)).toBeCloseTo(0, 1);
    // Due East
    expect(calculateBearing(0, 0, 0, 10)).toBeCloseTo(90, 1);
    // Due South
    expect(calculateBearing(10, 0, 0, 0)).toBeCloseTo(180, 1);
    // Due West
    expect(calculateBearing(0, 10, 0, 0)).toBeCloseTo(270, 1);
  });
});

describe('interpolateSample', () => {
  const SAMPLES: readonly ReplaySample[] = [
    { epochSec: 1000, lat: 10, lon: 20, altitude: 100 },
    { epochSec: 1100, lat: 12, lon: 24, altitude: 200 },
    { epochSec: 1200, lat: 14, lon: 28, altitude: 300 },
  ];

  it('returns null for empty samples', () => {
    expect(interpolateSample([], 1050)).toBeNull();
  });

  it('returns single sample for 1-sample list', () => {
    const first = SAMPLES[0];
    expect(first).toBeDefined();
    if (!first) return;
    const res = interpolateSample([first], 1050);
    expect(res).toEqual({
      lat: 10,
      lon: 20,
      altitude: 100,
      currentSampleIndex: 0,
    });
  });

  it('clamps before first sample and after last sample', () => {
    const before = interpolateSample(SAMPLES, 900);
    expect(before?.lat).toBe(10);
    expect(before?.currentSampleIndex).toBe(0);

    const after = interpolateSample(SAMPLES, 1500);
    expect(after?.lat).toBe(14);
    expect(after?.currentSampleIndex).toBe(2);
  });

  it('interpolates precisely halfway between samples', () => {
    const mid = interpolateSample(SAMPLES, 1050);
    expect(mid?.lat).toBeCloseTo(11, 4);
    expect(mid?.lon).toBeCloseTo(22, 4);
    expect(mid?.altitude).toBeCloseTo(150, 4);
    expect(mid?.currentSampleIndex).toBe(0);
  });

  it('handles antimeridian crossing', () => {
    const crossingSamples: readonly ReplaySample[] = [
      { epochSec: 1000, lat: 0, lon: 179 },
      { epochSec: 1100, lat: 0, lon: -179 },
    ];

    const mid = interpolateSample(crossingSamples, 1050);
    expect(mid?.lat).toBe(0);
    // Midpoint between 179 and -179 is 180 or -180
    expect(Math.abs(mid?.lon ?? 0)).toBeCloseTo(180, 2);
  });
});
