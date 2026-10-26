import type { EvidencePack, SceneCameraPose, SceneShot } from '../../contracts';

/** Pure TS port of `sampleCameraMove` (legacy/src/director/camera.js). Angles are degrees. */
export interface CameraMove {
  readonly from: SceneCameraPose;
  readonly to: SceneCameraPose;
  readonly easing?: 'linear' | 'cubic';
}

/** Wrap a longitude to [-180, 180) without touching authored endpoints. */
export function normalizeLongitude(longitude: number): number {
  return ((((longitude + 180) % 360) + 360) % 360) - 180;
}

/** Samples the authored curve, taking the short arc for longitude and angles. */
export function sampleCameraMove(move: CameraMove, progress: number): SceneCameraPose {
  const t = Math.max(0, Math.min(1, Number(progress) || 0));
  if (t === 0) return { ...move.from };
  if (t === 1) return { ...move.to };
  const eased = move.easing === 'linear' ? t : t < 0.5 ? 4 * t ** 3 : 1 - (-2 * t + 2) ** 3 / 2;
  const lerp = (a: number, b: number) => a + (b - a) * eased;
  const angle = (a: number, b: number) => a + (((((b - a + 540) % 360) + 360) % 360) - 180) * eased;
  const { from, to } = move;
  return {
    lat: lerp(from.lat, to.lat),
    lon: normalizeLongitude(angle(from.lon, to.lon)),
    altM: lerp(from.altM, to.altM),
    headingDeg: angle(from.headingDeg, to.headingDeg),
    pitchDeg: lerp(from.pitchDeg, to.pitchDeg),
    rollDeg: angle(from.rollDeg, to.rollDeg),
  };
}

// ── playback state machine ───────────────────────────────────────────────────────────

export type Phase = 'idle' | 'flying' | 'holding' | 'paused' | 'finished';

export interface PlaybackState {
  readonly phase: Phase;
  readonly index: number;
  /** Phase that `paused` resumes into. */
  readonly resumePhase: 'flying' | 'holding';
  /** Milliseconds left in the current flying/holding phase. */
  readonly remainingMs: number;
}

export interface Clock {
  now(): number;
  setTimeout(fn: () => void, ms: number): unknown;
  clearTimeout(handle: unknown): void;
}

export const systemClock: Clock = {
  now: () => Date.now(),
  setTimeout: (fn, ms) => setTimeout(fn, ms),
  clearTimeout: (h) => clearTimeout(h as ReturnType<typeof setTimeout>),
};

const ms = (s: number) => Math.max(0, Math.round(s * 1000));

/**
 * Plays an evidence pack's shots: fly (durationS) then hold (holdS), then advance.
 * Time comes from an injectable clock; `onShot` fires whenever a shot begins (play, next, prev).
 */
export class ScenePlayer {
  readonly #shots: readonly SceneShot[];
  readonly #clock: Clock;
  readonly #onShot: (shot: SceneShot, index: number) => void;
  readonly #listeners = new Set<() => void>();
  #state: PlaybackState = { phase: 'idle', index: 0, resumePhase: 'flying', remainingMs: 0 };
  #timer: unknown = null;
  #deadline = 0;

  constructor(
    pack: Pick<EvidencePack, 'shots'>,
    onShot: (shot: SceneShot, index: number) => void,
    clock: Clock = systemClock,
  ) {
    this.#shots = pack.shots;
    this.#onShot = onShot;
    this.#clock = clock;
  }

  get state(): PlaybackState {
    return this.#state;
  }

  subscribe(listener: () => void): () => void {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }

  /** Starts from the beginning when idle/finished; resumes when paused. */
  play(): void {
    if (this.#shots.length === 0) return;
    const s = this.#state;
    if (s.phase === 'flying' || s.phase === 'holding') return;
    if (s.phase === 'paused') {
      this.#enter(s.resumePhase, s.index, s.remainingMs);
      return;
    }
    this.#begin(s.phase === 'finished' ? 0 : s.index);
  }

  pause(): void {
    const s = this.#state;
    if (s.phase !== 'flying' && s.phase !== 'holding') return;
    this.#cancel();
    this.#set({
      phase: 'paused',
      index: s.index,
      resumePhase: s.phase,
      remainingMs: Math.max(0, this.#deadline - this.#clock.now()),
    });
  }

  next(): void {
    if (this.#shots.length === 0) return;
    const i = this.#state.index + 1;
    if (i >= this.#shots.length) return;
    this.#begin(i);
  }

  prev(): void {
    if (this.#shots.length === 0) return;
    this.#begin(Math.max(0, this.#state.index - 1));
  }

  /** Jumps to and plays a given shot. */
  goTo(index: number): void {
    if (index < 0 || index >= this.#shots.length) return;
    this.#begin(index);
  }

  stop(): void {
    this.#cancel();
    this.#set({ phase: 'idle', index: 0, resumePhase: 'flying', remainingMs: 0 });
  }

  dispose(): void {
    this.#cancel();
    this.#listeners.clear();
  }

  #begin(index: number): void {
    const shot = this.#shots[index];
    if (!shot) return;
    this.#onShot(shot, index);
    this.#enter('flying', index, ms(shot.durationS));
  }

  #enter(phase: 'flying' | 'holding', index: number, remainingMs: number): void {
    this.#cancel();
    this.#deadline = this.#clock.now() + remainingMs;
    this.#timer = this.#clock.setTimeout(() => this.#elapsed(), remainingMs);
    this.#set({ phase, index, resumePhase: phase, remainingMs });
  }

  #elapsed(): void {
    this.#timer = null;
    const { phase, index } = this.#state;
    const shot = this.#shots[index];
    if (!shot) return;
    if (phase === 'flying') {
      this.#enter('holding', index, ms(shot.holdS));
    } else if (phase === 'holding') {
      if (index + 1 < this.#shots.length) this.#begin(index + 1);
      else this.#set({ phase: 'finished', index, resumePhase: 'flying', remainingMs: 0 });
    }
  }

  #cancel(): void {
    if (this.#timer !== null) this.#clock.clearTimeout(this.#timer);
    this.#timer = null;
  }

  #set(state: PlaybackState): void {
    this.#state = state;
    for (const l of [...this.#listeners]) l();
  }
}

/** Camera target for the globe's `flyTo`. */
export function shotFlyTarget(shot: SceneShot) {
  return {
    kind: 'point' as const,
    position: { lat: shot.pose.lat, lon: shot.pose.lon },
    heightM: shot.pose.altM,
  };
}

/** Layers the pack needs that are not currently enabled. */
export function missingLayers(
  pack: Pick<EvidencePack, 'requiredLayers'>,
  enabled: ReadonlySet<string>,
): string[] {
  return pack.requiredLayers.filter((l) => !enabled.has(l));
}

/** Strips markup from a catalog credit line; the result is plain text, safe to render. */
export function plainCredit(html: string): string {
  return html
    .replace(/<[^>]*>/g, '')
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/\s+/g, ' ')
    .trim();
}
