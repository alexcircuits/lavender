/**
 * Hazard lines (cyclone track and forecast cone) with the same two-tone edge as the hazard
 * point symbols: a core in the ramp colour, then a 1 px light keyline and a 1 px
 * `--map-outline` on each side, in one polyline (one Fabric material, no stacked lines that
 * would fight for depth).
 */
import type { Color } from '@cesium/core';
import { Material } from '@cesium/engine';
import { HAZARD_EDGE_PX, KEYLINE_PX } from './hazardShapes';

const TYPE = 'LavenderHazardKeylineLine';

// `v_width` is the polyline's width in CSS pixels (PolylineVS); `st.t` runs 0 → 1 across it.
const SOURCE = `
uniform vec4 color;
uniform vec4 keylineColor;
uniform vec4 outlineColor;
uniform float edgeWidth;
uniform float keylineWidth;

in float v_width;

czm_material czm_getMaterial(czm_materialInput materialInput)
{
    czm_material material = czm_getDefaultMaterial(materialInput);
    float fromCentre = abs(materialInput.st.t - 0.5) * v_width;
    float coreHalfWidth = 0.5 * v_width - edgeWidth;
    vec4 band = fromCentre <= coreHalfWidth
        ? color
        : (fromCentre <= coreHalfWidth + keylineWidth ? keylineColor : outlineColor);
    band = czm_gammaCorrect(band);
    material.diffuse = band.rgb;
    material.alpha = band.a;
    return material;
}
`;

/** Polyline width for a hazard line whose coloured core is `corePx` wide. */
export function keylineLineWidth(corePx: number): number {
  return corePx + 2 * HAZARD_EDGE_PX;
}

/** A hazard line material: `color` core, `keyline` then `outline` on both sides. */
export function keylineLineMaterial(color: Color, keyline: Color, outline: Color): Material {
  return new Material({
    fabric: {
      type: TYPE,
      uniforms: {
        color,
        keylineColor: keyline,
        outlineColor: outline,
        edgeWidth: HAZARD_EDGE_PX,
        keylineWidth: KEYLINE_PX,
      },
      source: SOURCE,
    },
    translucent: (material: Material) => material.uniforms.color.alpha < 1,
  });
}
