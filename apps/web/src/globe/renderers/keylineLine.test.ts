import { Color } from '@cesium/core';
import { describe, expect, it } from 'vitest';
import { DEFAULT_PALETTE } from '../tokens';
import { HAZARD_EDGE_PX } from './hazardShapes';
import { keylineLineMaterial, keylineLineWidth } from './keylineLine';
import { toMapColors } from './types';

describe('hazard keyline lines', () => {
  it('widens the coloured core by the hazard edge on both sides', () => {
    expect(keylineLineWidth(2)).toBe(2 + 2 * HAZARD_EDGE_PX);
    expect(keylineLineWidth(1)).toBe(1 + 2 * HAZARD_EDGE_PX);
  });

  it('carries the core, keyline and outline colours, translucent only when dimmed', () => {
    const colors = toMapColors(DEFAULT_PALETTE);
    const live = keylineLineMaterial(colors.hazardRamp[2], colors.keyline, colors.outline);
    expect(live.type).toBe('LavenderHazardKeylineLine');
    expect(Color.equals(live.uniforms.color, colors.hazardRamp[2])).toBe(true);
    expect(Color.equals(live.uniforms.keylineColor, colors.keyline)).toBe(true);
    expect(Color.equals(live.uniforms.outlineColor, colors.outline)).toBe(true);
    expect(live.isTranslucent()).toBe(false);
    const stale = keylineLineMaterial(
      colors.hazardRamp[2].withAlpha(0.6),
      colors.keyline.withAlpha(0.6),
      colors.outline.withAlpha(0.6),
    );
    expect(stale.isTranslucent()).toBe(true);
    // The shader source is shared by type: a second material does not redefine it.
    expect(stale.shaderSource).toContain('keylineColor');
  });
});
