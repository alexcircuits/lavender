import { describe, expect, it } from 'vitest';
import { litRatio } from './capture';

describe('litRatio', () => {
  it('counts pixels brighter than near-black', () => {
    const rgba = new Uint8ClampedArray([
      0, 0, 0, 255, 200, 200, 200, 255, 30, 20, 9, 255, 10, 60, 0, 255,
    ]);
    // Only the grey pixel (600) and the green-ish one (70) clear the threshold of 60.
    expect(litRatio(rgba)).toBe(0.5);
  });

  it('is zero for an empty frame', () => {
    expect(litRatio(new Uint8ClampedArray())).toBe(0);
  });
});
