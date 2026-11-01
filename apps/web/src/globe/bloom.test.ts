import { describe, expect, it } from 'vitest';
import {
  applyBloomToScene,
  BLOOM_INTENSITY_DEFAULT,
  BLOOM_INTENSITY_MAX,
  BLOOM_SCALE_VERSION,
  bloomStrengthFromIntensity,
  clampBloomIntensity,
  decodeBloomIntensity,
  legacyBloomToV2,
} from './bloom';

describe('bloom constants', () => {
  it('exports expected constants', () => {
    expect(BLOOM_SCALE_VERSION).toBe(2);
    expect(BLOOM_INTENSITY_MAX).toBe(200);
    expect(BLOOM_INTENSITY_DEFAULT).toBe(100);
  });
});

describe('clampBloomIntensity', () => {
  it('clamps values within range [0, 200]', () => {
    expect(clampBloomIntensity(0)).toBe(0);
    expect(clampBloomIntensity(100)).toBe(100);
    expect(clampBloomIntensity(200)).toBe(200);
  });

  it('clamps out-of-range values', () => {
    expect(clampBloomIntensity(-1)).toBe(0);
    expect(clampBloomIntensity(-50)).toBe(0);
    expect(clampBloomIntensity(201)).toBe(200);
    expect(clampBloomIntensity(999)).toBe(200);
  });

  it('rounds fractional numbers to nearest integer', () => {
    expect(clampBloomIntensity(50.4)).toBe(50);
    expect(clampBloomIntensity(50.6)).toBe(51);
    expect(clampBloomIntensity(199.9)).toBe(200);
  });

  it('falls back to default for non-finite values', () => {
    expect(clampBloomIntensity(Number.NaN)).toBe(BLOOM_INTENSITY_DEFAULT);
    expect(clampBloomIntensity(Number.POSITIVE_INFINITY)).toBe(BLOOM_INTENSITY_DEFAULT);
    expect(clampBloomIntensity(Number.NEGATIVE_INFINITY)).toBe(BLOOM_INTENSITY_DEFAULT);
  });
});

describe('legacyBloomToV2', () => {
  it('converts and un-inverts legacy v1 (0–100) scale to v2 (0–200) scale', () => {
    // In legacy v1: 100 = off (no bloom), 0 = max bloom
    expect(legacyBloomToV2(100)).toBe(0);
    expect(legacyBloomToV2(0)).toBe(200);
    expect(legacyBloomToV2(50)).toBe(100);
    expect(legacyBloomToV2(25)).toBe(150);
    expect(legacyBloomToV2(75)).toBe(50);
  });

  it('clamps inputs outside the legacy 0–100 range', () => {
    expect(legacyBloomToV2(-10)).toBe(200);
    expect(legacyBloomToV2(150)).toBe(0);
  });

  it('rounds fractional legacy inputs', () => {
    expect(legacyBloomToV2(49.6)).toBe(100); // 50 -> 100
  });

  it('handles non-finite values safely', () => {
    expect(legacyBloomToV2(Number.NaN)).toBe(200);
  });
});

describe('decodeBloomIntensity', () => {
  it('decodes version 2 persisted values directly', () => {
    expect(decodeBloomIntensity(0, 2)).toBe(0);
    expect(decodeBloomIntensity(100, 2)).toBe(100);
    expect(decodeBloomIntensity(200, 2)).toBe(200);
    expect(decodeBloomIntensity(250, 2)).toBe(200);
    expect(decodeBloomIntensity(-10, 2)).toBe(0);
  });

  it('supports future versions >= BLOOM_SCALE_VERSION', () => {
    expect(decodeBloomIntensity(140, 3)).toBe(140);
  });

  it('migrates explicit version 1 values from legacy inverted scale', () => {
    expect(decodeBloomIntensity(100, 1)).toBe(0);
    expect(decodeBloomIntensity(0, 1)).toBe(200);
    expect(decodeBloomIntensity(50, 1)).toBe(100);
    expect(decodeBloomIntensity(80, 1)).toBe(40);
  });

  it('tolerates unversioned links that already use the new 0..200 scale (> 100)', () => {
    expect(decodeBloomIntensity(150)).toBe(150);
    expect(decodeBloomIntensity(199)).toBe(199);
    expect(decodeBloomIntensity(250)).toBe(200);
  });

  it('treats unversioned links <= 100 as legacy v1', () => {
    expect(decodeBloomIntensity(100)).toBe(0);
    expect(decodeBloomIntensity(0)).toBe(200);
    expect(decodeBloomIntensity(50)).toBe(100);
  });

  it('returns default intensity for non-finite values', () => {
    expect(decodeBloomIntensity(Number.NaN)).toBe(BLOOM_INTENSITY_DEFAULT);
    expect(decodeBloomIntensity(Number.POSITIVE_INFINITY)).toBe(BLOOM_INTENSITY_DEFAULT);
  });
});

describe('bloomStrengthFromIntensity', () => {
  it('normalizes intensity to 0.0–1.0 range', () => {
    expect(bloomStrengthFromIntensity(0)).toBe(0.0);
    expect(bloomStrengthFromIntensity(100)).toBe(0.5);
    expect(bloomStrengthFromIntensity(200)).toBe(1.0);
  });

  it('clamps out-of-range inputs when normalizing', () => {
    expect(bloomStrengthFromIntensity(-20)).toBe(0.0);
    expect(bloomStrengthFromIntensity(300)).toBe(1.0);
  });
});

describe('applyBloomToScene', () => {
  const createMockScene = (initialEnabled = false) => {
    const bloom = {
      enabled: initialEnabled,
      uniforms: {
        contrast: 0,
        brightness: 0,
        delta: 0,
        sigma: 0,
        stepSize: 0,
        glowOnly: true,
      },
    };
    return {
      postProcessStages: {
        bloom,
      },
    };
  };

  it('safely ignores null or invalid scenes', () => {
    expect(() => applyBloomToScene(null, true, 100)).not.toThrow();
    expect(() => applyBloomToScene(undefined, true, 100)).not.toThrow();
    expect(() => applyBloomToScene({}, true, 100)).not.toThrow();
    expect(() => applyBloomToScene({ postProcessStages: {} }, true, 100)).not.toThrow();
  });

  it('initializes uniforms object if undefined on bloom stage', () => {
    const scene = {
      postProcessStages: {
        bloom: { enabled: false } as { enabled: boolean; uniforms?: Record<string, unknown> },
      },
    };
    applyBloomToScene(scene, true, 200);
    expect(scene.postProcessStages.bloom.uniforms).toBeDefined();
    expect(scene.postProcessStages.bloom.uniforms?.contrast).toBeCloseTo(87.0);
  });

  it('keeps bloom disabled when enabled is false', () => {
    const scene = createMockScene(true);
    applyBloomToScene(scene, false, 200);
    expect(scene.postProcessStages.bloom.enabled).toBe(false);
  });

  it('gates bloom stage when rawStrength <= 0.06 (cut-off threshold)', () => {
    const scene = createMockScene();

    // 0 intensity: rawStrength = 0 <= 0.06
    applyBloomToScene(scene, true, 0);
    expect(scene.postProcessStages.bloom.enabled).toBe(false);
    expect(scene.postProcessStages.bloom.uniforms.contrast).toBe(255.0);
    expect(scene.postProcessStages.bloom.uniforms.brightness).toBe(-0.5);
    expect(scene.postProcessStages.bloom.uniforms.sigma).toBe(0.28);
    expect(scene.postProcessStages.bloom.uniforms.delta).toBe(0.2);
    expect(scene.postProcessStages.bloom.uniforms.stepSize).toBe(1.0);
    expect(scene.postProcessStages.bloom.uniforms.glowOnly).toBe(false);

    // 12 intensity: rawStrength = 12 / 200 = 0.06 (not > 0.06)
    applyBloomToScene(scene, true, 12);
    expect(scene.postProcessStages.bloom.enabled).toBe(false);

    // 13 intensity: rawStrength = 13 / 200 = 0.065 > 0.06
    applyBloomToScene(scene, true, 13);
    expect(scene.postProcessStages.bloom.enabled).toBe(true);
  });

  it('applies cubic smoothstep easing to uniforms at maximum intensity (200)', () => {
    const scene = createMockScene();
    applyBloomToScene(scene, true, 200);

    expect(scene.postProcessStages.bloom.enabled).toBe(true);
    // At intensity 200: rawStrength = 1.0, strength = 1.0, eased = 1.0
    expect(scene.postProcessStages.bloom.uniforms.contrast).toBeCloseTo(255.0 - 168.0); // 87.0
    expect(scene.postProcessStages.bloom.uniforms.brightness).toBeCloseTo(-0.5 + 0.36); // -0.14
    expect(scene.postProcessStages.bloom.uniforms.sigma).toBeCloseTo(0.28 + 6.3); // 6.58
    expect(scene.postProcessStages.bloom.uniforms.delta).toBeCloseTo(0.2 + 2.25); // 2.45
    expect(scene.postProcessStages.bloom.uniforms.stepSize).toBeCloseTo(1.0 + 1.25); // 2.25
    expect(scene.postProcessStages.bloom.uniforms.glowOnly).toBe(false);
  });

  it('applies Hermite cubic smoothstep easing at mid intensity (100)', () => {
    const scene = createMockScene();
    applyBloomToScene(scene, true, 100);

    expect(scene.postProcessStages.bloom.enabled).toBe(true);
    const rawStrength = 100 / 200; // 0.5
    const strength = (rawStrength - 0.06) / 0.94;
    const eased = strength * strength * (3.0 - 2.0 * strength);

    expect(scene.postProcessStages.bloom.uniforms.contrast).toBeCloseTo(255.0 - eased * 168.0);
    expect(scene.postProcessStages.bloom.uniforms.brightness).toBeCloseTo(-0.5 + eased * 0.36);
    expect(scene.postProcessStages.bloom.uniforms.sigma).toBeCloseTo(0.28 + eased * 6.3);
    expect(scene.postProcessStages.bloom.uniforms.delta).toBeCloseTo(0.2 + eased * 2.25);
    expect(scene.postProcessStages.bloom.uniforms.stepSize).toBeCloseTo(1.0 + eased * 1.25);
  });
});
