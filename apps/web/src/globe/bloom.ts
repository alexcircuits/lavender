/**
 * bloom.ts — Cesium Bloom & HDR Post-Process Normalization (LB-162).
 *
 * The bloom slider was rescaled from 0–100 (v1) to 0–200 (v2) to provide
 * finer control. Share links and saved scene projects may carry either scale,
 * so all ingestion paths pass through {@link decodeBloomIntensity} which
 * detects the version and converts to the v2 range.
 */

/** Current bloom scale version. Persisted in share links / scene projects. */
export const BLOOM_SCALE_VERSION = 2;

/** Maximum bloom intensity on the v2 scale. */
export const BLOOM_INTENSITY_MAX = 200;

/** Default bloom intensity. */
export const BLOOM_INTENSITY_DEFAULT = 100;

/** Maximum intensity on the legacy v1 scale (used for migration). */
const LEGACY_BLOOM_INTENSITY_MAX = 100;

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

/**
 * Clamp a raw bloom value to the valid v2 range [0, 200].
 * @param value — raw intensity (may be out of range or NaN)
 * @returns integer in [0, BLOOM_INTENSITY_MAX]
 */
export function clampBloomIntensity(value: number): number {
  const num = Number(value);
  if (!Number.isFinite(num)) return BLOOM_INTENSITY_DEFAULT;
  return clamp(Math.round(num), 0, BLOOM_INTENSITY_MAX);
}

/**
 * Convert a v1 (0–100) bloom value to the v2 (0–200) scale.
 * The v1 scale was inverted (100 = no bloom, 0 = max bloom),
 * so this also un-inverts.
 * @param value — legacy v1 intensity
 * @returns equivalent v2 intensity
 */
export function legacyBloomToV2(value: number): number {
  const num = Number(value);
  const legacy = clamp(Math.round(Number.isFinite(num) ? num : 0), 0, LEGACY_BLOOM_INTENSITY_MAX);
  return clampBloomIntensity((LEGACY_BLOOM_INTENSITY_MAX - legacy) * 2);
}

/**
 * Decode a bloom intensity from a share link or saved project.
 * Handles both v1 (0–100, inverted) and v2 (0–200) scales.
 * @param value — persisted intensity
 * @param version — bloom scale version from the saved state (defaults to 1)
 * @returns v2-scale intensity in [0, 200]
 */
export function decodeBloomIntensity(value: number, version = 1): number {
  const num = Number(value);
  if (!Number.isFinite(num)) return BLOOM_INTENSITY_DEFAULT;

  const normalizedVersion = Number(version);
  if (Number.isFinite(normalizedVersion) && normalizedVersion >= BLOOM_SCALE_VERSION) {
    return clampBloomIntensity(num);
  }

  // Tolerate unversioned links/projects that already use the new 0..200 scale.
  if (num > LEGACY_BLOOM_INTENSITY_MAX) {
    return clampBloomIntensity(num);
  }

  return legacyBloomToV2(num);
}

/**
 * Convert a bloom intensity (0–200) to a normalized strength (0.0–1.0)
 * suitable for passing to the CesiumJS bloom post-process stage.
 * @param value — v2-scale intensity
 * @returns normalized strength in [0, 1]
 */
export function bloomStrengthFromIntensity(value: number): number {
  return clampBloomIntensity(value) / BLOOM_INTENSITY_MAX;
}

interface BloomUniforms {
  contrast?: number;
  brightness?: number;
  delta?: number;
  sigma?: number;
  stepSize?: number;
  glowOnly?: boolean;
}

interface BloomStageLike {
  enabled: boolean;
  uniforms?: BloomUniforms;
}

interface CesiumSceneLike {
  postProcessStages?: {
    bloom?: BloomStageLike;
  };
}

/**
 * Apply normalized bloom settings and Hermite cubic smoothstep easing to Cesium's bloom post-process stage.
 *
 * @param scene — Cesium scene or mock scene
 * @param enabled — whether bloom is enabled
 * @param intensity — bloom intensity in [0, 200]
 */
export function applyBloomToScene(scene: unknown, enabled: boolean, intensity: number): void {
  if (!scene || typeof scene !== 'object') return;
  const stages = (scene as CesiumSceneLike).postProcessStages;
  if (!stages || typeof stages !== 'object') return;
  const bloomStage = stages.bloom;
  if (!bloomStage || typeof bloomStage !== 'object') return;

  const clampedIntensity = clampBloomIntensity(intensity);
  const rawStrength = bloomStrengthFromIntensity(clampedIntensity);
  const strength = rawStrength <= 0.06 ? 0.0 : (rawStrength - 0.06) / 0.94;
  const eased = strength * strength * (3.0 - 2.0 * strength);

  if (!bloomStage.uniforms || typeof bloomStage.uniforms !== 'object') {
    bloomStage.uniforms = {};
  }

  bloomStage.uniforms.contrast = 255.0 - eased * 168.0;
  bloomStage.uniforms.brightness = -0.5 + eased * 0.36;
  bloomStage.uniforms.sigma = 0.28 + eased * 6.3;
  bloomStage.uniforms.delta = 0.2 + eased * 2.25;
  bloomStage.uniforms.stepSize = 1.0 + eased * 1.25;
  bloomStage.uniforms.glowOnly = false;
  bloomStage.enabled = Boolean(enabled && rawStrength > 0.06);
}
