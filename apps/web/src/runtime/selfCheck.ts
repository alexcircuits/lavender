/**
 * The desktop self-check (docs/architecture/tauri.md §8): once the default layer has
 * delivered live data through the platform and the globe is attached, the app reports what
 * it saw. On desktop the report goes to `smoke_report`, which only writes it when automated
 * verification set `LAVENDER_SMOKE_OUT`; nothing leaves the machine.
 */
import type { FeedId } from '../contracts';
import type { DiagnosticsSink } from '../platform';
import type { Runtime } from './createRuntime';

export interface SelfCheckOptions {
  /** The feed whose live data proves the end-to-end path. */
  readonly feed?: FeedId;
  /** Give up and report failure after this long. */
  readonly timeoutMs?: number;
  /** Let the globe render the data before reporting. */
  readonly settleMs?: number;
  /** Where browser-level problems are observed (tests pass a stub). */
  readonly target?: Pick<Window, 'addEventListener' | 'removeEventListener'>;
}

export interface SelfCheckReport extends Record<string, unknown> {
  readonly ok: boolean;
  readonly platform: string;
  readonly feed: FeedId;
  readonly count: number;
  readonly freshness: string | null;
  readonly transport: string;
  readonly globeAttached: boolean;
  readonly layers: number;
  readonly errors: readonly string[];
  readonly cspViolations: readonly string[];
  readonly elapsedMs: number;
  /** Share of lit pixels in the captured frame; null when the globe could not be read back. */
  readonly litRatio: number | null;
  /** The captured frame as a PNG data URL, as visual evidence for automated runs. */
  readonly frame: string | null;
  /** Animation frames the page received in one second: 0 means the WebView is not rendering
   *  (an occluded or hidden window), which explains a missing frame. */
  readonly framesPerSecond: number;
  readonly visibility: string;
}

/** Counts `requestAnimationFrame` callbacks over `ms` of wall-clock time. */
function countAnimationFrames(ms: number): Promise<number> {
  return new Promise((resolve) => {
    let frames = 0;
    let running = true;
    const tick = (): void => {
      if (!running) return;
      frames += 1;
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
    setTimeout(() => {
      running = false;
      resolve(frames);
    }, ms);
  });
}

/** Width of the evidence frame in the report. */
const FRAME_WIDTH = 640;
/** Below this the globe is considered blank (the home view fills far more of the frame). */
const MIN_LIT_RATIO = 0.02;

export function startSelfCheck(
  runtime: Runtime,
  sink: DiagnosticsSink,
  options: SelfCheckOptions = {},
): () => void {
  const feed = options.feed ?? 'earthquakes';
  const timeoutMs = options.timeoutMs ?? 60_000;
  const settleMs = options.settleMs ?? 3_000;
  const target = options.target ?? window;
  const started = performance.now();
  const errors: string[] = [];
  const cspViolations: string[] = [];
  const { feeds, ui, system } = runtime.stores;
  let done = false;
  let settleTimer: ReturnType<typeof setTimeout> | null = null;

  const onError = (event: Event): void => {
    const message =
      event instanceof ErrorEvent
        ? event.message
        : 'reason' in event
          ? String(event.reason)
          : event.type;
    errors.push(message.slice(0, 300));
  };
  const onCsp = (event: Event): void => {
    const e = event as SecurityPolicyViolationEvent;
    cspViolations.push(`${e.violatedDirective} ${e.blockedURI}`.slice(0, 300));
  };
  target.addEventListener('error', onError);
  target.addEventListener('unhandledrejection', onError);
  target.addEventListener('securitypolicyviolation', onCsp);

  const finish = async (ok: boolean): Promise<void> => {
    if (done) return;
    done = true;
    const framesPerSecond = await countAnimationFrames(1_000);
    const frame = await (
      ui.getState().globe?.captureFrame(FRAME_WIDTH) ?? Promise.resolve(null)
    ).catch(() => null);
    cleanup();
    const summary = feeds.getState().summaries[feed];
    const rendered = frame !== null && frame.litRatio >= MIN_LIT_RATIO;
    const report: SelfCheckReport = {
      ok: ok && rendered && errors.length === 0 && cspViolations.length === 0,
      platform: runtime.platform.kind,
      feed,
      count: summary?.count ?? 0,
      freshness: summary?.freshness?.state ?? null,
      transport: feeds.getState().transport,
      globeAttached: ui.getState().globe !== null,
      layers: system.getState().catalog?.layers.length ?? 0,
      errors,
      cspViolations,
      elapsedMs: Math.round(performance.now() - started),
      litRatio: frame?.litRatio ?? null,
      framesPerSecond,
      visibility: typeof document === 'undefined' ? 'unknown' : document.visibilityState,
      frame: frame?.dataUrl ?? null,
    };
    sink.report(report).catch(() => {
      // Diagnostics are best effort; the app keeps running either way.
    });
  };

  const check = (): void => {
    if (done || settleTimer) return;
    const summary = feeds.getState().summaries[feed];
    if (summary?.phase === 'ready' && summary.count > 0 && ui.getState().globe) {
      settleTimer = setTimeout(() => void finish(true), settleMs);
    }
  };

  const unsubscribers = [feeds.subscribe(check), ui.subscribe(check)];
  const timeout = setTimeout(() => void finish(false), timeoutMs);
  function cleanup(): void {
    for (const u of unsubscribers) u();
    clearTimeout(timeout);
    if (settleTimer) clearTimeout(settleTimer);
    target.removeEventListener('error', onError);
    target.removeEventListener('unhandledrejection', onError);
    target.removeEventListener('securitypolicyviolation', onCsp);
  }
  check();
  return () => {
    done = true;
    cleanup();
  };
}
