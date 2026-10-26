/**
 * Celestial reference ring (LB-160): the screen-space ecliptic horizon of upstream
 * `legacy/src/celestialRing.js`, ported to one canvas-2D overlay above the Cesium canvas.
 * Geometry and projection math live in `features/celestial/model.ts` (tested there); this
 * module owns the Cesium ephemeris sampling (Simon1994 + the ICRF-to-fixed rotation), the
 * per-frame camera projection, the hysteresis visibility gate and the drawing itself.
 *
 * The ring is visible only while the whole Earth disc sits inside the outer keyhole
 * (24 px enter / 12 px exit clearance, so zooming does not flicker it). Sun rays, the moon
 * haze and the tapered marker arcs are expensive (gradients + shadows), so they are cached
 * in two offscreen layers re-rendered only when the ring geometry changes, then composited
 * rotated by each body's screen bearing — the same two-layer treatment as upstream, minus
 * its per-layer DOM canvases.
 *
 * Colours come from CSS custom properties via `TokenReader` (canvas fillStyle strings
 * assembled from tokens are the established pattern, see hazardGlyphs.ts). Unlike upstream
 * the setting is sticky: the overlay only hides while the camera is inside the keyhole
 * composition; enabling the ring does not fly the camera (upstream's focusFullGlobe and
 * auto-disable remain a follow-up).
 */
import { Cartesian2, Cartesian3, Ellipsoid, JulianDate, Matrix3 } from '@cesium/core';
import type { Scene } from '@cesium/engine';
import { SceneTransforms, Simon1994PlanetaryPositions, Transforms } from '@cesium/engine';
import {
  celestialScreenAngle,
  circularAngleDistance,
  earthDiscScreenRadius,
  isFullGlobeInsideKeyhole,
  type KeyholeGeometry,
  type ScreenAngle,
} from '../../features/celestial/model';
import type { TokenReader } from '../tokens';

/** Outer edge of the keyhole in normalized viewport space (upstream lockstep). */
export const KEYHOLE_OUTER_RADIUS = 1.05;
/** Gap between the keyhole edge and the drawn ring. */
export const RING_INSET_PX = 11;
/** Gap between the ring and the Sun/Moon markers riding it. */
export const MARKER_INSET_PX = 36;
/** Extra radial lane for the Moon when the markers are near conjunction. */
export const COLLIDING_MARKER_EXTRA_INSET_PX = 32;
/** Gap between the Earth limb and the inner edge of the sun-ray annulus. */
export const RAY_INNER_CLEARANCE_PX = 12;
/** Bearances closer than this (rad) put the Moon into the second radial lane. */
export const MARKER_COLLISION_ANGLE = (6 * Math.PI) / 180;
/** This overlay is a secondary HUD treatment: at most 30 redraws/s. */
export const CELESTIAL_MAX_FRAME_RATE = 30;
/** Per-layer backing-store budget (two effect layers share the main canvas' size). */
export const CELESTIAL_MAX_BACKING_PIXELS = 2_000_000;
export const CELESTIAL_MAX_BACKING_DIMENSION = 1_600;
export const CELESTIAL_MAX_DEVICE_PIXEL_RATIO = 1.25;
/** Ephemeris resample period: camera motion only re-projects the cached vectors. */
export const EPHEMERIS_PERIOD_MS = 60_000;

/** A plain world-direction triple; Cesium Cartesian3 satisfies it structurally. */
export interface Vec3 {
  readonly x: number;
  readonly y: number;
  readonly z: number;
}

/** The shared screen-space keyhole circle, or null for degenerate viewports. */
export function keyholeGeometryFor(
  widthPx: number,
  heightPx: number,
): { readonly centerX: number; readonly centerY: number; readonly radius: number } | null {
  if (!(widthPx > 0) || !(heightPx > 0)) return null;
  return {
    centerX: widthPx * 0.5,
    centerY: heightPx * 0.5,
    radius: heightPx * 0.5 * KEYHOLE_OUTER_RADIUS,
  };
}

export interface DiscGeometryInput {
  /** Camera distance from the Earth centre, metres. */
  readonly cameraDistanceM: number;
  readonly viewportWidthPx: number;
  readonly viewportHeightPx: number;
  /** Camera vertical field of view, radians. */
  readonly fovyRad: number;
  /** Projected Earth-centre position in CSS pixels (window coordinates). */
  readonly earthCenterScreenX: number;
  readonly earthCenterScreenY: number;
}

/**
 * The full keyhole-containment geometry for a camera: the projected Earth disc from
 * `earthDiscScreenRadius` against the shared keyhole circle. Null when the viewport or the
 * disc is degenerate (camera inside/at the surface, invalid FOV, empty viewport).
 */
export function discGeometry(input: DiscGeometryInput): KeyholeGeometry | null {
  const keyhole = keyholeGeometryFor(input.viewportWidthPx, input.viewportHeightPx);
  if (!keyhole) return null;
  const earthRadius = earthDiscScreenRadius(
    input.cameraDistanceM,
    input.viewportHeightPx,
    input.fovyRad,
  );
  if (earthRadius === null) return null;
  return {
    earthCenterX: input.earthCenterScreenX,
    earthCenterY: input.earthCenterScreenY,
    earthRadius,
    keyholeCenterX: keyhole.centerX,
    keyholeCenterY: keyhole.centerY,
    keyholeRadius: keyhole.radius,
  };
}

export interface RingLayout {
  /** Drawn ring radius in CSS pixels. */
  readonly radius: number;
  /** Inner edge of the sun-ray annulus (never inside the Earth limb). */
  readonly rayInnerRadius: number;
  /** Radial lane of the Sun/Moon markers. */
  readonly markerRadius: number;
}

/** Ring radii from the keyhole and the projected Earth limb; null inside the inset. */
export function ringLayout(keyholeRadiusPx: number, earthRadiusPx: number): RingLayout | null {
  if (!Number.isFinite(keyholeRadiusPx) || !Number.isFinite(earthRadiusPx)) return null;
  const radius = keyholeRadiusPx - RING_INSET_PX;
  if (radius <= 0) return null;
  const rayInnerRadius = Math.min(
    radius - RAY_INNER_CLEARANCE_PX,
    earthRadiusPx + Math.max(38, radius * 0.065),
  );
  return { radius, rayInnerRadius, markerRadius: radius - MARKER_INSET_PX };
}

/**
 * Camera-plane bearing of a celestial body: projects `direction` onto the camera's
 * right/up axes and defers to the model's stable-angle rule (faded prior bearing while
 * the vector points into/out of the camera).
 */
export function screenBearing(
  direction: Vec3,
  rightWC: Vec3,
  upWC: Vec3,
  lastAngle: number,
): ScreenAngle {
  const rightComponent =
    direction.x * rightWC.x + direction.y * rightWC.y + direction.z * rightWC.z;
  const upComponent = direction.x * upWC.x + direction.y * upWC.y + direction.z * upWC.z;
  return celestialScreenAngle(rightComponent, upComponent, lastAngle);
}

/** True when Sun and Moon markers would overlap on the ring (near conjunction). */
export function markersCollide(sunAngle: number, moonAngle: number): boolean {
  return Math.abs(circularAngleDistance(sunAngle, moonAngle)) < MARKER_COLLISION_ANGLE;
}

/**
 * `"r, g, b"` triple from a CSS colour, for `rgba()` assembly on the canvas. Handles the
 * hex and `rgb()` forms tokens.css uses; null for anything else (keywords, color-mix()).
 */
export function rgbTripleOf(color: string): string | null {
  const value = color.trim().toLowerCase();
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/.exec(value);
  const digits = hex?.[1];
  if (digits) {
    const full = digits.length === 3 ? digits.replaceAll(/./g, (d) => d + d) : digits;
    return `${Number.parseInt(full.slice(0, 2), 16)}, ${Number.parseInt(full.slice(2, 4), 16)}, ${Number.parseInt(full.slice(4, 6), 16)}`;
  }
  const rgb = /^rgba?\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})/.exec(value);
  if (rgb) {
    const [r, g, b] = [rgb[1], rgb[2], rgb[3]];
    if (r !== undefined && g !== undefined && b !== undefined) {
      const red = Number(r);
      const green = Number(g);
      const blue = Number(b);
      if (red <= 255 && green <= 255 && blue <= 255) {
        return `${red}, ${green}, ${blue}`;
      }
    }
  }
  return null;
}

export interface CelestialInk {
  /** Ring outline + cardinal ticks, as `"r, g, b"` triples. */
  readonly ring: string;
  readonly tick: string;
  /** Sun rays and marker (warm), moon haze and marker (cool). */
  readonly sun: string;
  readonly moon: string;
}

const FALLBACK_INK: CelestialInk = {
  ring: '214, 211, 230', // --map-space
  tick: '163, 168, 180', // --ink-2
  sun: '240, 163, 58', // --warning
  moon: '56, 189, 248', // --map-boundary-maritime
};

/** Ink for the overlay from CSS custom properties, each with its token default. */
export function readCelestialInk(read: TokenReader): CelestialInk {
  return {
    ring: rgbTripleOf(read('--map-space')) ?? FALLBACK_INK.ring,
    tick: rgbTripleOf(read('--ink-2')) ?? FALLBACK_INK.tick,
    sun: rgbTripleOf(read('--warning')) ?? FALLBACK_INK.sun,
    moon: rgbTripleOf(read('--map-boundary-maritime')) ?? FALLBACK_INK.moon,
  };
}

type Context2D = CanvasRenderingContext2D;

/** Draw one tapered orbital arc around a celestial marker (upstream port). */
export function drawTaperedArc(
  ctx: Context2D,
  cx: number,
  cy: number,
  radius: number,
  angle: number,
  rgb: string,
  strength: number,
): void {
  const span = 0.82;
  const segments = 18;
  ctx.save();
  ctx.lineCap = 'round';
  ctx.shadowColor = `rgba(${rgb}, ${0.5 * strength})`;
  ctx.shadowBlur = 9;
  for (let i = 0; i < segments; i++) {
    const t0 = i / segments;
    const t1 = (i + 1) / segments;
    const a0 = angle - span + span * 2 * t0;
    const a1 = angle - span + span * 2 * t1;
    const envelope = Math.sin(Math.PI * ((t0 + t1) * 0.5));
    ctx.strokeStyle = `rgba(${rgb}, ${strength * envelope * envelope})`;
    ctx.lineWidth = 0.7 + envelope * 1.25;
    ctx.beginPath();
    ctx.arc(cx, cy, radius, a0, a1);
    ctx.stroke();
  }
  ctx.restore();
}

/** Draw the soft directional rays cast inward from the sun marker (upstream port). */
export function drawSunRays(
  ctx: Context2D,
  cx: number,
  cy: number,
  radius: number,
  innerRadius: number,
  angle: number,
  rgb: string,
): void {
  const sx = cx + Math.cos(angle) * radius;
  const sy = cy + Math.sin(angle) * radius;
  ctx.save();
  // The reference keeps the globe untouched: all illumination lives in the annulus
  // between the Earth limb and the outer optics ring.
  ctx.beginPath();
  ctx.arc(cx, cy, radius + 3, 0, Math.PI * 2);
  ctx.arc(cx, cy, innerRadius, 0, Math.PI * 2, true);
  ctx.clip('evenodd');
  ctx.globalCompositeOperation = 'screen';
  const glow = ctx.createRadialGradient(sx, sy, 4, sx, sy, radius * 0.94);
  glow.addColorStop(0, `rgba(${rgb}, 0.17)`);
  glow.addColorStop(0.22, `rgba(${rgb}, 0.085)`);
  glow.addColorStop(0.58, `rgba(${rgb}, 0.027)`);
  glow.addColorStop(1, `rgba(${rgb}, 0)`);
  ctx.fillStyle = glow;
  ctx.fillRect(cx - radius - 4, cy - radius - 4, (radius + 4) * 2, (radius + 4) * 2);

  ctx.lineCap = 'round';
  ctx.shadowColor = `rgba(${rgb}, 0.11)`;
  ctx.shadowBlur = 18;
  const annulusWidth = Math.max(1, radius - innerRadius);
  for (let i = -2; i <= 2; i++) {
    const spread = i * 0.043;
    const rayAngle = angle + Math.PI + spread;
    const rayLength = annulusWidth * (0.7 + (2 - Math.abs(i)) * 0.07);
    const ex = sx + Math.cos(rayAngle) * rayLength;
    const ey = sy + Math.sin(rayAngle) * rayLength;
    const lineGradient = ctx.createLinearGradient(sx, sy, ex, ey);
    lineGradient.addColorStop(0, `rgba(${rgb}, ${0.075 - Math.abs(i) * 0.011})`);
    lineGradient.addColorStop(1, `rgba(${rgb}, 0)`);
    ctx.strokeStyle = lineGradient;
    ctx.lineWidth = i === 0 ? 1.8 : 0.9;
    ctx.beginPath();
    ctx.moveTo(sx, sy);
    ctx.lineTo(ex, ey);
    ctx.stroke();
  }
  ctx.restore();
}

/** Draw a restrained cool haze around the moon sector (upstream port). */
export function drawMoonHaze(
  ctx: Context2D,
  cx: number,
  cy: number,
  radius: number,
  angle: number,
  rgb: string,
): void {
  const mx = cx + Math.cos(angle) * radius;
  const my = cy + Math.sin(angle) * radius;
  const haze = ctx.createRadialGradient(mx, my, 0, mx, my, radius * 0.22);
  haze.addColorStop(0, `rgba(${rgb}, 0.16)`);
  haze.addColorStop(0.42, `rgba(${rgb}, 0.055)`);
  haze.addColorStop(1, `rgba(${rgb}, 0)`);
  ctx.save();
  ctx.globalCompositeOperation = 'screen';
  ctx.fillStyle = haze;
  ctx.beginPath();
  ctx.arc(mx, my, radius * 0.22, 0, Math.PI * 2);
  ctx.fill();
  ctx.restore();
}

/** The static ring face: the faint outline circle plus screen-cardinal ticks. */
export function drawRingFace(
  ctx: Context2D,
  cx: number,
  cy: number,
  radius: number,
  ink: CelestialInk,
): void {
  ctx.save();
  ctx.strokeStyle = `rgba(${ink.ring}, 0.095)`;
  ctx.lineWidth = 0.8;
  ctx.beginPath();
  ctx.arc(cx, cy, radius, 0, Math.PI * 2);
  ctx.stroke();

  const major = Math.min(14, Math.max(8, radius * 0.045));
  const minor = major * 0.6;
  for (let i = 0; i < 8; i++) {
    const angle = (i * Math.PI) / 4;
    const isMajor = i % 2 === 0;
    const length = isMajor ? major : minor;
    const cos = Math.cos(angle);
    const sin = Math.sin(angle);
    ctx.strokeStyle = `rgba(${ink.tick}, ${isMajor ? 0.5 : 0.28})`;
    ctx.lineWidth = isMajor ? 1.2 : 0.8;
    ctx.beginPath();
    ctx.moveTo(cx + cos * (radius - length), cy + sin * (radius - length));
    ctx.lineTo(cx + cos * radius, cy + sin * radius);
    ctx.stroke();
  }
  ctx.restore();
}

/** One small body marker riding the ring: a disc with a soft halo. */
function drawMarker(
  ctx: Context2D,
  cx: number,
  cy: number,
  markerRadius: number,
  angle: number,
  rgb: string,
  opacity: number,
): void {
  const x = cx + Math.cos(angle) * markerRadius;
  const y = cy + Math.sin(angle) * markerRadius;
  ctx.save();
  ctx.globalAlpha = opacity;
  ctx.fillStyle = `rgba(${rgb}, 0.9)`;
  ctx.beginPath();
  ctx.arc(x, y, 4.5, 0, Math.PI * 2);
  ctx.fill();
  ctx.strokeStyle = `rgba(${rgb}, 0.35)`;
  ctx.lineWidth = 2;
  ctx.beginPath();
  ctx.arc(x, y, 7.5, 0, Math.PI * 2);
  ctx.stroke();
  ctx.restore();
}

interface EffectLayer {
  readonly canvas: HTMLCanvasElement;
  readonly ctx: Context2D | null;
}

function makeLayer(): EffectLayer {
  const canvas = document.createElement('canvas');
  return { canvas, ctx: canvas.getContext('2d') };
}

export interface CelestialRingOptions {
  /** Requests the one frame whose postRender repaints the ring (explicit render mode). */
  readonly requestRender: () => void;
  /** CSS custom-property reader; defaults to the document root (globe/tokens.ts). */
  readonly readToken?: TokenReader | undefined;
}

export interface CelestialRingDebugState {
  readonly enabled: boolean;
  readonly visible: boolean;
  readonly sunAngle: number;
  readonly moonAngle: number;
  readonly sunOpacity: number;
  readonly moonOpacity: number;
  readonly ephemerisUpdates: number;
  /** Backing-store scale of the budget-capped canvases. */
  readonly renderScale: number;
  readonly disc: KeyholeGeometry | null;
}

/**
 * The celestial ring overlay. Attach with `new CelestialRingRenderer(scene, options)`,
 * toggle with `setEnabled`, release with `destroy`. The postRender draw is fully guarded:
 * a throw is logged once per frame and never reaches Cesium's render loop.
 */
export class CelestialRingRenderer {
  readonly #scene: Scene;
  readonly #options: CelestialRingOptions;
  readonly #ink: CelestialInk;
  readonly #root: HTMLDivElement;
  readonly #canvas: HTMLCanvasElement;
  readonly #ctx: Context2D | null;
  readonly #sunLayer: EffectLayer;
  readonly #moonLayer: EffectLayer;
  readonly #scratch = {
    sunInertial: new Cartesian3(),
    moonInertial: new Cartesian3(),
    sunFixed: new Cartesian3(),
    moonFixed: new Cartesian3(),
    toCenter: new Cartesian3(),
    screenCenter: new Cartesian2(),
    fixedMatrix: new Matrix3(),
  };
  #enabled = false;
  #visible = false;
  #sunAngle = 0;
  #moonAngle = Math.PI;
  #sunOpacity = 1;
  #moonOpacity = 1;
  #ephemerisDirty = true;
  #ephemerisUpdates = 0;
  #nextDrawAt = 0;
  #renderScale = 1;
  #backingWidth = 0;
  #backingHeight = 0;
  #sunRenderKey = '';
  #moonRenderKey = '';
  #disc: KeyholeGeometry | null = null;
  readonly #removePostRender: () => void;
  #ephemerisTimer: ReturnType<typeof setInterval> | null = null;

  constructor(scene: Scene, options: CelestialRingOptions) {
    this.#scene = scene;
    this.#options = options;
    this.#ink = readCelestialInk(options.readToken ?? defaultTokenReader);
    this.#root = document.createElement('div');
    this.#root.className = 'lv-celestial-ring';
    this.#root.setAttribute('aria-hidden', 'true');
    this.#root.style.cssText =
      'position:absolute;inset:0;overflow:hidden;pointer-events:none;z-index:1;';
    this.#canvas = document.createElement('canvas');
    this.#canvas.style.cssText =
      'position:absolute;inset:0;display:block;transform-origin:50% 50%;';
    this.#ctx = this.#canvas.getContext('2d');
    this.#sunLayer = makeLayer();
    this.#moonLayer = makeLayer();
    this.#root.append(this.#canvas);
    const container = scene.canvas.parentElement;
    if (container) container.appendChild(this.#root);
    this.#removePostRender = scene.postRender.addEventListener(() => this.#draw());
  }

  /**
   * Ephemeris staleness fix carried over from upstream: resample real wall time each
   * minute and request the one frame that repaints the ring. Started only while enabled —
   * upstream's always-running timer returns immediately when disabled.
   */
  #startEphemerisTimer(): void {
    if (this.#ephemerisTimer !== null) return;
    this.#ephemerisTimer = setInterval(() => {
      if (!this.#enabled) return;
      this.#ephemerisDirty = true;
      if (typeof document !== 'undefined' && document.hidden) return;
      this.#options.requestRender();
    }, EPHEMERIS_PERIOD_MS);
  }

  #stopEphemerisTimer(): void {
    if (this.#ephemerisTimer !== null) {
      clearInterval(this.#ephemerisTimer);
      this.#ephemerisTimer = null;
    }
  }

  /** Enable or disable the overlay (the user preference). */
  setEnabled(enabled: boolean): void {
    const was = this.#enabled;
    this.#enabled = enabled;
    if (enabled && !was) {
      this.#ephemerisDirty = true;
      this.#startEphemerisTimer();
    }
    if (!enabled) {
      this.#visible = false;
      this.#stopEphemerisTimer();
      this.#clear();
    }
    this.#root.classList.toggle('disabled', !enabled);
    if (enabled && !was) this.#options.requestRender();
  }

  /** Read-only geometry snapshot for browser QA. */
  getDebugState(): CelestialRingDebugState {
    return {
      enabled: this.#enabled,
      visible: this.#visible,
      sunAngle: this.#sunAngle,
      moonAngle: this.#moonAngle,
      sunOpacity: this.#sunOpacity,
      moonOpacity: this.#moonOpacity,
      ephemerisUpdates: this.#ephemerisUpdates,
      renderScale: this.#renderScale,
      disc: this.#disc ? { ...this.#disc } : null,
    };
  }

  /** Detach the render hook, stop the ephemeris timer and remove the overlay DOM. */
  destroy(): void {
    this.#stopEphemerisTimer();
    this.#removePostRender();
    this.#root.remove();
  }

  #clear(): void {
    this.#ctx?.clearRect(0, 0, this.#canvas.width, this.#canvas.height);
    this.#sunLayer.ctx?.clearRect(0, 0, this.#sunLayer.canvas.width, this.#sunLayer.canvas.height);
    this.#moonLayer.ctx?.clearRect(
      0,
      0,
      this.#moonLayer.canvas.width,
      this.#moonLayer.canvas.height,
    );
    this.#sunRenderKey = '';
    this.#moonRenderKey = '';
  }

  /**
   * Resize the backing stores (budget-capped like upstream) while drawing in CSS pixels.
   * The engine renders 1:1 CSS pixels, so the cap also bounds the overlay's fill rate.
   */
  #resize(width: number, height: number): void {
    const nativeDpr = Math.max(1, window.devicePixelRatio || 1);
    const pixelsScale = Math.sqrt(CELESTIAL_MAX_BACKING_PIXELS / (width * height));
    const dimensionScale = CELESTIAL_MAX_BACKING_DIMENSION / Math.max(width, height);
    const dpr = Math.min(nativeDpr, CELESTIAL_MAX_DEVICE_PIXEL_RATIO, pixelsScale, dimensionScale);
    const bw = Math.max(1, Math.round(width * dpr));
    const bh = Math.max(1, Math.round(height * dpr));
    for (const layer of [this.#canvas, this.#sunLayer.canvas, this.#moonLayer.canvas]) {
      if (layer.width !== bw || layer.height !== bh) {
        layer.width = bw;
        layer.height = bh;
      }
    }
    this.#canvas.style.width = `${width}px`;
    this.#canvas.style.height = `${height}px`;
    this.#ctx?.setTransform(dpr, 0, 0, dpr, 0, 0);
    this.#sunLayer.ctx?.setTransform(dpr, 0, 0, dpr, 0, 0);
    this.#moonLayer.ctx?.setTransform(dpr, 0, 0, dpr, 0, 0);
    this.#renderScale = dpr;
    this.#backingWidth = width;
    this.#backingHeight = height;
  }

  /**
   * Sample Earth-fixed Sun/Moon directions once per enable (and once a minute). Camera
   * movement only re-projects these cached vectors; it never re-runs the planetary model.
   */
  #updateEphemeris(time: JulianDate): boolean {
    if (!this.#ephemerisDirty) return true;
    const { sunInertial, moonInertial, sunFixed, moonFixed, fixedMatrix } = this.#scratch;
    Simon1994PlanetaryPositions.computeSunPositionInEarthInertialFrame(time, sunInertial);
    Simon1994PlanetaryPositions.computeMoonPositionInEarthInertialFrame(time, moonInertial);
    const matrix =
      Transforms.computeIcrfToFixedMatrix(time, fixedMatrix) ||
      Transforms.computeTemeToPseudoFixedMatrix(time, fixedMatrix);
    if (!matrix) return false;
    Matrix3.multiplyByVector(matrix, sunInertial, sunFixed);
    Matrix3.multiplyByVector(matrix, moonInertial, moonFixed);
    Cartesian3.normalize(sunFixed, sunFixed);
    Cartesian3.normalize(moonFixed, moonFixed);
    this.#ephemerisDirty = false;
    this.#ephemerisUpdates += 1;
    return true;
  }

  /** The projected Earth disc + keyhole containment geometry for the current camera. */
  #projectedDisc(width: number, height: number): KeyholeGeometry | null {
    const camera = this.#scene.camera;
    const distance = Cartesian3.magnitude(camera.positionWC);
    if (!Number.isFinite(distance) || distance <= Ellipsoid.WGS84.maximumRadius) return null;
    const { toCenter, screenCenter } = this.#scratch;
    Cartesian3.negate(camera.positionWC, toCenter);
    if (Cartesian3.dot(camera.directionWC, toCenter) <= 0) return null;
    // World-to-window of the Earth centre (upstream projectEarthDiscToViewport).
    let projected: Cartesian2 | undefined;
    try {
      projected = SceneTransforms.worldToWindowCoordinates(
        this.#scene,
        Cartesian3.ZERO,
        screenCenter,
      );
    } catch {
      return null;
    }
    if (!projected || !Number.isFinite(projected.x) || !Number.isFinite(projected.y)) return null;
    // Only the perspective frustum has a vertical FOV; the others never gate the ring.
    const frustum = camera.frustum;
    const fovy = frustum && 'fovy' in frustum ? frustum.fovy : Number.NaN;
    return discGeometry({
      cameraDistanceM: distance,
      viewportWidthPx: width,
      viewportHeightPx: height,
      fovyRad: fovy ?? Number.NaN,
      earthCenterScreenX: projected.x,
      earthCenterScreenY: projected.y,
    });
  }

  /** Re-render the cached sun/moon effect layers when the geometry key changes. */
  #renderEffectLayers(cx: number, cy: number, radius: number, rayInnerRadius: number): void {
    const outlineKey = `${cx}:${cy}:${Math.round(radius)}:${Math.round(rayInnerRadius)}`;
    const sunCtx = this.#sunLayer.ctx;
    if (sunCtx && outlineKey !== this.#sunRenderKey) {
      sunCtx.clearRect(0, 0, this.#sunLayer.canvas.width, this.#sunLayer.canvas.height);
      drawSunRays(sunCtx, cx, cy, radius, rayInnerRadius, 0, this.#ink.sun);
      drawTaperedArc(sunCtx, cx, cy, radius, 0, this.#ink.sun, 0.56);
      this.#sunRenderKey = outlineKey;
    }
    const moonCtx = this.#moonLayer.ctx;
    if (moonCtx && outlineKey !== this.#moonRenderKey) {
      moonCtx.clearRect(0, 0, this.#moonLayer.canvas.width, this.#moonLayer.canvas.height);
      drawMoonHaze(moonCtx, cx, cy, radius, 0, this.#ink.moon);
      drawTaperedArc(moonCtx, cx, cy, radius, 0, this.#ink.moon, 0.42);
      this.#moonRenderKey = outlineKey;
    }
  }

  /** Composite the cached layers onto the visible canvas at the current bearings. */
  #compose(cx: number, cy: number, layout: RingLayout): void {
    const ctx = this.#ctx;
    if (!ctx) return;
    const width = this.#backingWidth;
    const height = this.#backingHeight;
    ctx.clearRect(0, 0, this.#canvas.width, this.#canvas.height);
    drawRingFace(ctx, cx, cy, layout.radius, this.#ink);
    this.#drawRotatedLayer(
      ctx,
      this.#sunLayer,
      cx,
      cy,
      width,
      height,
      this.#sunAngle,
      this.#sunOpacity,
    );
    this.#drawRotatedLayer(
      ctx,
      this.#moonLayer,
      cx,
      cy,
      width,
      height,
      this.#moonAngle,
      this.#moonOpacity,
    );
    const collide = markersCollide(this.#sunAngle, this.#moonAngle);
    const moonRadius = layout.markerRadius - (collide ? COLLIDING_MARKER_EXTRA_INSET_PX : 0);
    drawMarker(ctx, cx, cy, layout.markerRadius, this.#sunAngle, this.#ink.sun, this.#sunOpacity);
    drawMarker(ctx, cx, cy, moonRadius, this.#moonAngle, this.#ink.moon, this.#moonOpacity);
  }

  #drawRotatedLayer(
    ctx: Context2D,
    layer: EffectLayer,
    cx: number,
    cy: number,
    width: number,
    height: number,
    angle: number,
    alpha: number,
  ): void {
    if (layer.canvas.width === 0 || layer.canvas.height === 0) return;
    ctx.save();
    ctx.globalAlpha = alpha;
    ctx.translate(cx, cy);
    ctx.rotate(angle);
    ctx.drawImage(layer.canvas, -cx, -cy, width, height);
    ctx.restore();
  }

  /** Per-frame draw from postRender: gate, project, composite. Never throws. */
  #draw(): void {
    if (!this.#enabled || !this.#ctx) return;
    const now = performance.now();
    if (now < this.#nextDrawAt) return;
    this.#nextDrawAt = now + 1000 / CELESTIAL_MAX_FRAME_RATE;
    try {
      if (this.#scene.isDestroyed?.()) return;
      const canvas = this.#scene.canvas;
      const width = canvas.clientWidth || canvas.width;
      const height = canvas.clientHeight || canvas.height;
      if (!(width > 0 && height > 0)) return;
      this.#resize(width, height);

      const disc = this.#projectedDisc(width, height);
      this.#disc = disc;
      const nextVisible = disc ? isFullGlobeInsideKeyhole(disc, this.#visible) : false;
      this.#visible = nextVisible;
      if (!nextVisible || !disc) {
        this.#clear();
        return;
      }
      if (!this.#updateEphemeris(JulianDate.now())) return;

      const camera = this.#scene.camera;
      const sun = screenBearing(
        this.#scratch.sunFixed,
        camera.rightWC,
        camera.upWC,
        this.#sunAngle,
      );
      const moon = screenBearing(
        this.#scratch.moonFixed,
        camera.rightWC,
        camera.upWC,
        this.#moonAngle,
      );
      if (sun.stable) this.#sunAngle = sun.angle;
      if (moon.stable) this.#moonAngle = moon.angle;
      this.#sunOpacity = sun.opacity;
      this.#moonOpacity = moon.opacity;

      const cx = width * 0.5;
      const cy = height * 0.5;
      const layout = ringLayout(disc.keyholeRadius, disc.earthRadius);
      if (!layout) return;
      this.#renderEffectLayers(cx, cy, layout.radius, layout.rayInnerRadius);
      this.#compose(cx, cy, layout);
    } catch (error) {
      // The overlay must never break Cesium's render loop; degrade to blank.
      console.error('Celestial ring could not draw', error);
    }
  }
}

function defaultTokenReader(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}
