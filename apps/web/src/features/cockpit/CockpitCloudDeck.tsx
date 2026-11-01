import { useEffect, useRef } from 'react';
import {
  deriveWeatherEffectProfile,
  type WeatherEffectInput,
  type WeatherEffectProfile,
  weatherAltitudeFactors,
} from '../../globe/renderers/wasmSkyKinematics';
import { cssTokenReader } from '../../globe/tokens';
import { usePlatform } from '../../runtime';
import {
  CLOUD_STRENGTH_BLEND,
  CLOUD_VISIBLE_THRESHOLD,
  type CloudAnchorPoint,
  type CloudDeckLayer,
  cloudBandCount,
  cloudBandOffsets,
  cloudDarkness,
  cloudDeckLayers,
  cloudDriftRate,
  cloudFogVeil,
  cloudFrameIntervalMs,
  cloudStrength,
  cockpitCloudRefreshDue,
  cockpitCloudRenderSize,
  WEATHER_REFRESH_CHECK_MS,
} from './cloudDeck';
import styles from './cloudDeck.module.css';
import { fetchWeatherObservation, type WeatherObservationProvider } from './weatherObservation';

export interface CockpitCloudDeckProps {
  readonly className?: string | undefined;
  /**
   * The camera anchor: the tracked aircraft's position while cockpit mode is
   * active. Null (or missing coordinates) disables weather fetches; the deck
   * then eases out and stays clear.
   */
  readonly anchor?: (CloudAnchorPoint & { readonly altitudeM?: number }) | null | undefined;
  /** Injectable observation source; defaults to the Open-Meteo provider. */
  readonly weatherProvider?: WeatherObservationProvider | undefined;
}

interface DeckTokens {
  readonly cloudShadow: string;
  readonly cloudLit: string;
  readonly fog: string;
  readonly darkness: string;
}

/** Design-token colours with the validated palette as fallback (tokens.ts rule). */
function readDeckTokens(): DeckTokens {
  const read = cssTokenReader();
  return {
    cloudShadow: read('--map-context') || '#9a98a8',
    cloudLit: read('--map-keyline') || '#bcbcc4',
    fog: read('--map-space') || '#d6d3e6',
    // --scrim as flat RGB; the veil opacity rides on globalAlpha.
    darkness: read('--scrim') || '#060010',
  };
}

/**
 * Cockpit cloud deck (LB-161): the weather-backed 2.5D cloud overlay of upstream
 * `legacy/src/cockpitCloudEffects.js`, re-rendered as a layered canvas-2D deck.
 * Mounted only while cockpit mode is active and the persisted preference is on.
 *
 * The deck fetches an observation for the camera anchor (5 min stationary /
 * 25 km moved, upstream lockstep), maps it through the WASM weather kernel
 * (`lavender_weather_derive_effects`) into effect strengths, and composites
 * three parallax cloud bands plus a fog veil and storm darkness from design
 * tokens. Renders at 12–15 fps driven by the kernel's frame-budget hint —
 * heavy weather spends fewer, not heavier, frames — and clears below the
 * visibility threshold. Honours prefers-reduced-motion (static, 1 Hz) and
 * pauses while the document is hidden.
 */
export function CockpitCloudDeck({ className, anchor, weatherProvider }: CockpitCloudDeckProps) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const anchorRef = useRef(anchor);
  anchorRef.current = anchor;
  const platform = usePlatform();

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    // The observation rides the platform boundary (ADR 0005): the injected
    // provider wins (tests), otherwise the Open-Meteo-backed platform call.
    const provider = weatherProvider ?? fetchWeatherObservation(platform);
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const tokens = readDeckTokens();
    const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

    // Mutable render state (the upstream controller fields, narrowed to what
    // the canvas deck needs).
    let profile: WeatherEffectProfile | null = null;
    let strength = 0;
    let targetStrength = 0;
    let frameIntervalMs = cloudFrameIntervalMs(0);
    let lastFrameMs = Number.NEGATIVE_INFINITY;
    let lastRefreshCheckMs = Number.NEGATIVE_INFINITY;
    let fetchedAt = 0;
    let weatherAnchor: CloudAnchorPoint | null = null;
    let canvasCleared = true;
    let suspended = document.hidden;
    let disposed = false;
    let rafId: number | null = null;
    let motionTimer: ReturnType<typeof setTimeout> | null = null;
    /** While an observation is in flight no refresh may start another one. */
    let pending = false;
    const abort = new AbortController();

    const applyObservation = (observation: WeatherEffectInput, altitudeM: number): void => {
      const next = deriveWeatherEffectProfile(observation);
      profile = next;
      const altitude = weatherAltitudeFactors(altitudeM);
      targetStrength = cloudStrength(next, altitude.cloud);
      frameIntervalMs = cloudFrameIntervalMs(next.frameCost);
      canvas.dataset.cloudStrength = targetStrength.toFixed(3);
      canvas.dataset.weatherCode = String(observation.weatherCode ?? 'unknown');
      if (reducedMotion) {
        // Upstream reduced-motion path: snap and render one static frame.
        strength = targetStrength;
        canvas.dataset.active = String(strength > CLOUD_VISIBLE_THRESHOLD);
        renderFrame(0);
      }
    };

    const refresh = async (): Promise<void> => {
      // Re-entrancy guard (upstream `this.pending`): the 1 Hz refresh check must
      // not stack a second observation while one is still in flight, and the
      // unmount abort must not be re-observed by a late tick.
      if (disposed || suspended || pending) return;
      const point = anchorRef.current;
      if (!point || !Number.isFinite(point.latitude) || !Number.isFinite(point.longitude)) return;
      const due = cockpitCloudRefreshDue({
        nowMs: Date.now(),
        fetchedAt,
        anchor: weatherAnchor,
        point,
        hasWeather: profile?.available === true,
      });
      if (!due) return;
      pending = true;
      try {
        const observation = await provider(point.latitude, point.longitude, abort.signal);
        if (disposed || abort.signal.aborted || !observation) return;
        weatherAnchor = { latitude: point.latitude, longitude: point.longitude };
        fetchedAt = Date.now();
        applyObservation(observation, Math.max(0, point.altitudeM ?? 0));
      } catch {
        // Observation unavailable: the deck keeps its last profile and eases
        // out on the following strength steps (upstream fails soft too).
      } finally {
        pending = false;
      }
    };

    const drawBand = (layer: CloudDeckLayer, x: number, y: number, radius: number): void => {
      // Soft edge: a wide faint shadow disc under a tighter lit core disc.
      ctx.fillStyle = tokens.cloudShadow;
      ctx.globalAlpha = layer.density * strength * 0.34;
      ctx.beginPath();
      ctx.arc(x, y, radius, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillStyle = tokens.cloudLit;
      ctx.globalAlpha = layer.density * strength * 0.3;
      ctx.beginPath();
      ctx.arc(x - radius * 0.12, y - radius * 0.18, radius * 0.62, 0, Math.PI * 2);
      ctx.fill();
      ctx.globalAlpha = 1;
    };

    const renderFrame = (timeSec: number): void => {
      if (!profile?.available) return;
      const width = canvas.width;
      const height = canvas.height;
      ctx.globalCompositeOperation = 'source-over';
      ctx.globalAlpha = 1;
      ctx.clearRect(0, 0, width, height);
      canvasCleared = false;
      const layers = cloudDeckLayers(profile);
      for (let i = 0; i < layers.length; i++) {
        const layer = layers[i];
        if (!layer || layer.density <= 0.001) continue;
        const offsets = cloudBandOffsets(
          i * 17 + 3,
          cloudBandCount(layer.density),
          timeSec * cloudDriftRate(profile.wind, profile.windDirectionDeg, layer.drift),
        );
        const radius = width * 0.14 * layer.depth;
        for (let band = 0; band < offsets.length; band++) {
          const x = (offsets[band] ?? 0) * width;
          // Deterministic golden-ratio vertical jitter so bands never stack.
          const y = (0.14 + layer.altitude * 0.6 + ((band * 0.618) % 0.22)) * height;
          drawBand(layer, x, y, radius);
        }
      }

      // Fog veil: token-tinted wash over the whole deck.
      const fog = cloudFogVeil(profile) * strength;
      if (fog > 0.001) {
        ctx.fillStyle = tokens.fog;
        ctx.globalAlpha = Math.min(0.55, fog * 0.55);
        ctx.fillRect(0, 0, width, height);
      }

      // Storm darkness: the token scrim, capped below the PFD's legibility.
      const darkness = cloudDarkness(profile) * strength;
      if (darkness > 0.001) {
        ctx.fillStyle = tokens.darkness;
        ctx.globalAlpha = Math.min(0.62, darkness * 0.62);
        ctx.fillRect(0, 0, width, height);
      }

      // Visor fade (upstream smoothstep(-0.82, -0.28, uv.y) simplified): erase
      // the bottom of the frame so clouds sit above the cockpit sill.
      ctx.globalCompositeOperation = 'destination-out';
      ctx.globalAlpha = 1;
      ctx.fillStyle = '#000000';
      ctx.fillRect(0, height * 0.78, width, height * 0.22);
      ctx.globalCompositeOperation = 'source-over';
      ctx.globalAlpha = 1;
    };

    const tick = (timeMs: number): void => {
      if (disposed || suspended) return;
      if (timeMs - lastRefreshCheckMs >= WEATHER_REFRESH_CHECK_MS) {
        lastRefreshCheckMs = timeMs;
        void refresh();
      }
      strength += (targetStrength - strength) * (reducedMotion ? 1 : CLOUD_STRENGTH_BLEND);
      const nowVisible = strength > CLOUD_VISIBLE_THRESHOLD;
      canvas.dataset.active = String(nowVisible);
      const renderDue = timeMs - lastFrameMs >= (reducedMotion ? 1000 : frameIntervalMs);
      if (nowVisible && renderDue) {
        lastFrameMs = timeMs;
        renderFrame(timeMs / 1000);
      } else if (!nowVisible && !canvasCleared) {
        // Faded out: clear once instead of every frame (upstream cleared here).
        ctx.globalCompositeOperation = 'source-over';
        ctx.globalAlpha = 1;
        ctx.clearRect(0, 0, canvas.width, canvas.height);
        canvasCleared = true;
        lastFrameMs = Number.NEGATIVE_INFINITY;
      }
      scheduleNext();
    };

    const scheduleNext = (): void => {
      if (disposed || suspended) return;
      if (reducedMotion) {
        // Reduced motion walks the loop at 1 Hz on a timer, not rAF.
        motionTimer = setTimeout(() => tick(performance.now()), 1000);
      } else {
        rafId = requestAnimationFrame(tick);
      }
    };

    const resize = (): void => {
      const host = canvas.parentElement;
      const width = host?.clientWidth ?? window.innerWidth;
      const height = host?.clientHeight ?? window.innerHeight;
      const size = cockpitCloudRenderSize(width, height);
      if (canvas.width === size.width && canvas.height === size.height) return;
      canvas.width = size.width;
      canvas.height = size.height;
      canvasCleared = true;
      renderFrame(0);
    };

    const onVisibility = (): void => {
      // Hidden-state gate (upstream perf wave 2): pause every scheduling chain
      // while hidden; resume exactly one on return.
      if (document.hidden === suspended) return;
      suspended = document.hidden;
      if (suspended) {
        if (rafId !== null) cancelAnimationFrame(rafId);
        rafId = null;
        if (motionTimer !== null) clearTimeout(motionTimer);
        motionTimer = null;
      } else {
        scheduleNext();
      }
    };

    resize();
    window.addEventListener('resize', resize);
    document.addEventListener('visibilitychange', onVisibility);
    if (!suspended) scheduleNext();

    return () => {
      disposed = true;
      abort.abort();
      if (rafId !== null) cancelAnimationFrame(rafId);
      if (motionTimer !== null) clearTimeout(motionTimer);
      window.removeEventListener('resize', resize);
      document.removeEventListener('visibilitychange', onVisibility);
    };
  }, [weatherProvider, platform]);

  return (
    <div
      className={`${styles.deck} ${className ?? ''}`}
      aria-hidden="true"
      data-testid="cockpit-cloud-deck"
    >
      <canvas ref={canvasRef} className={styles.canvas} />
    </div>
  );
}
