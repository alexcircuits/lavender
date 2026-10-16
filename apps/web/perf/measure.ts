/**
 * Node-side helpers of the browser performance harness: page setup with the Chrome DevTools
 * Protocol, bounded frame sampling, scripted camera motion and the heap. Every wait has a
 * time limit; a limit that runs out is reported in the results, not hidden.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import type { APIRequestContext, Browser, CDPSession, Page } from '@playwright/test';
import { instrument, type PerfState, type PerfWindow, type StreamFrame } from './instrument.ts';

/** GEV's capture conditions (legacy/docs/PERFORMANCE.md): 1440×900 at DPR 1. */
export const VIEWPORT = { width: 1440, height: 900 } as const;
/** GEV sampled 5 s of scripted motion and 5 s at rest. */
export const SAMPLE_MS = 5_000;
const LIVE_IMAGERY = process.env.LAVENDER_PERF_LIVE_IMAGERY === '1';

export interface Session {
  readonly page: Page;
  readonly cdp: CDPSession;
  close(): Promise<void>;
}

/**
 * A fresh context (cold browser cache) with the network cache disabled, as in GEV's startup
 * samples. GIBS imagery tiles are answered locally with a 512×512 synthetic tile unless
 * `LAVENDER_PERF_LIVE_IMAGERY=1`.
 */
export async function openSession(
  browser: Browser,
  baseURL: string,
  fixtures: string,
): Promise<Session> {
  const context = await browser.newContext({ baseURL, viewport: VIEWPORT, deviceScaleFactor: 1 });
  const page = await context.newPage();
  await page.addInitScript(instrument);
  const cdp = await context.newCDPSession(page);
  await cdp.send('Network.enable');
  await cdp.send('Network.setCacheDisabled', { cacheDisabled: true });
  await cdp.send('Performance.enable');
  if (!LIVE_IMAGERY) {
    const tile = readFileSync(join(fixtures, 'tile.png'));
    await page.route('https://gibs.earthdata.nasa.gov/**', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'image/png',
        headers: { 'Access-Control-Allow-Origin': '*', 'Timing-Allow-Origin': '*' },
        body: tile,
      }),
    );
  }
  return { page, cdp, close: () => context.close() };
}

export const imageryMode = (): string =>
  LIVE_IMAGERY
    ? 'live NASA GIBS tiles (network)'
    : 'GIBS answered locally with a synthetic 512×512 PNG';

export function perfState(page: Page): Promise<PerfState> {
  return page.evaluate(() => {
    const state = (window as PerfWindow).__lavenderPerf;
    if (!state) throw new Error('performance instrumentation is not installed');
    return state;
  });
}

/** The WebGL renderer the page would get (a separate page, so the measured one is untouched). */
export async function gpuInfo(browser: Browser): Promise<{ vendor: string; renderer: string }> {
  const page = await browser.newPage();
  try {
    return await page.evaluate(() => {
      const gl = document.createElement('canvas').getContext('webgl2');
      const info = gl?.getExtension('WEBGL_debug_renderer_info');
      if (!gl || !info) return { vendor: 'unavailable', renderer: 'unavailable' };
      return {
        vendor: String(gl.getParameter(info.UNMASKED_VENDOR_WEBGL)),
        renderer: String(gl.getParameter(info.UNMASKED_RENDERER_WEBGL)),
      };
    });
  } finally {
    await page.close();
  }
}

/** GEV's first control: reject software-rendered or unavailable GPU strings. */
export function isSoftwareRenderer(renderer: string): boolean {
  return /swiftshader|llvmpipe|software|unavailable|basic render/i.test(renderer);
}

export interface HeapSample {
  readonly usedMiB: number;
  readonly usedAfterGcMiB: number;
}

/** JS heap via `Performance.getMetrics`, as found and after a forced garbage collection. */
export async function heap(cdp: CDPSession): Promise<HeapSample> {
  const used = async () => {
    const { metrics } = await cdp.send('Performance.getMetrics');
    const value = metrics.find((m) => m.name === 'JSHeapUsedSize')?.value ?? Number.NaN;
    return Math.round((value / 2 ** 20) * 10) / 10;
  };
  const usedMiB = await used();
  await cdp.send('HeapProfiler.collectGarbage');
  return { usedMiB, usedAfterGcMiB: await used() };
}

export interface FrameSample {
  readonly durationMs: number;
  /** Frames the globe rendered (animation frames with WebGL draw calls). */
  readonly frames: number;
  readonly fps: number;
  /** Animation frames the browser ran: the display ceiling for `fps`. */
  readonly rafPerS: number;
  /** 95th percentile gap between rendered frames, ms (null below two frames). */
  readonly p95FrameGapMs: number | null;
  /** Stream frames that arrived during the window (they can trigger renders). */
  readonly streamFrames: ReadonlyArray<{ feed: string | null; bytes: number; handlerMs: number }>;
}

/** Counts rendered frames for exactly as long as `during` runs (or `ms` at rest). */
export async function sampleFrames(
  page: Page,
  ms: number,
  during?: () => Promise<void>,
): Promise<FrameSample> {
  const before = await page.evaluate(() => {
    const s = (window as PerfWindow).__lavenderPerf;
    return { at: performance.now(), raf: s?.rafTicks ?? 0 };
  });
  if (during) await during();
  else await page.waitForTimeout(ms);
  return page.evaluate((start) => {
    const s = (window as PerfWindow).__lavenderPerf;
    const end = performance.now();
    const times = (s?.frameTimes ?? []).filter((t) => t > start.at && t <= end);
    const gaps = times
      .slice(1)
      .map((t, i) => t - (times[i] ?? t))
      .sort((a, b) => a - b);
    const p95 =
      gaps.length > 0
        ? (gaps[Math.min(gaps.length - 1, Math.ceil(gaps.length * 0.95) - 1)] ?? null)
        : null;
    const durationMs = end - start.at;
    const round = (v: number) => Math.round(v * 10) / 10;
    return {
      durationMs: round(durationMs),
      frames: times.length,
      fps: round((times.length * 1000) / durationMs),
      rafPerS: round((((s?.rafTicks ?? 0) - start.raf) * 1000) / durationMs),
      p95FrameGapMs: p95 === null ? null : round(p95),
      streamFrames: (s?.frames ?? [])
        .filter((f) => f.at > start.at && f.at <= end)
        .map((f) => ({ feed: f.feed, bytes: f.bytes, handlerMs: round(f.handlerMs) })),
    };
  }, before);
}

/** Drags the globe in a circle around the canvas centre for `ms` of wall-clock time. */
export async function spin(page: Page, ms: number): Promise<void> {
  const box = await page.locator('.lv-globe-host canvas').boundingBox();
  if (!box) throw new Error('the globe canvas is not visible');
  const cx = box.x + box.width / 2;
  const cy = box.y + box.height / 2;
  const radius = Math.min(box.width, box.height) / 6;
  await page.mouse.move(cx + radius, cy);
  await page.mouse.down();
  const started = Date.now();
  while (Date.now() - started < ms) {
    const angle = ((Date.now() - started) / 2_000) * 2 * Math.PI;
    await page.mouse.move(cx + radius * Math.cos(angle), cy + radius * Math.sin(angle));
  }
  await page.mouse.up();
}

/** Motion samples per scenario: one 5 s sample (GEV) varied too much on a busy machine. */
export const MOTION_SAMPLES = 3;
/**
 * Motion before the samples, reported but not one of them: the first seconds of motion in
 * a new scene pay one-off costs (tile and shader loads for the views it passes).
 */
const WARM_UP_MS = 2_000;
/** Cesium's camera inertia has died down within a second of releasing the drag. */
const INERTIA_MS = 1_000;

/**
 * GEV's rest window (5 s), a 2 s motion warm-up, GEV's 5 s motion window repeated
 * `MOTION_SAMPLES` times, then the heap. About 26 s: it fits between two 30 s fixture
 * refreshes when started after one.
 */
export async function restAndMotion(page: Page, cdp: CDPSession) {
  const rest = await sampleFrames(page, SAMPLE_MS);
  const warmUp = await sampleFrames(page, WARM_UP_MS, () => spin(page, WARM_UP_MS));
  await page.waitForTimeout(INERTIA_MS);
  const motion: FrameSample[] = [];
  for (let i = 0; i < MOTION_SAMPLES; i += 1) {
    motion.push(await sampleFrames(page, SAMPLE_MS, () => spin(page, SAMPLE_MS)));
    // Let inertia stop before the next sample or the heap reading.
    await page.waitForTimeout(INERTIA_MS);
  }
  const fps = motion.map((m) => m.fps).sort((a, b) => a - b);
  return {
    rest,
    warmUpFps: warmUp.fps,
    motion,
    motionMedianFps: fps[Math.floor(fps.length / 2)] ?? null,
    heap: await heap(cdp),
  };
}

/**
 * Waits until the globe has rendered nothing for `quietMs` (bounded by `limitMs`) and returns
 * the time of the last rendered frame, or null when the limit ran out first.
 */
export async function settle(page: Page, quietMs: number, limitMs: number): Promise<number | null> {
  const deadline = Date.now() + limitMs;
  while (Date.now() < deadline) {
    const { last, now } = await page.evaluate(() => {
      const times = (window as PerfWindow).__lavenderPerf?.frameTimes ?? [];
      return { last: times.at(-1) ?? 0, now: performance.now() };
    });
    if (now - last >= quietMs) return last;
    await page.waitForTimeout(100);
  }
  return null;
}

/**
 * The first stream frame for `feed` after page time `after`, waiting at most `limitMs`.
 * With `newData`, only a frame whose `fetchedAt` differs from it counts: a new upstream
 * read, not a re-send of data the page already has.
 */
export async function nextStreamFrame(
  page: Page,
  feed: string,
  after: number,
  limitMs: number,
  newData?: { readonly since: string | null },
): Promise<StreamFrame | null> {
  const deadline = Date.now() + limitMs;
  const since = newData ? newData.since : undefined;
  while (Date.now() < deadline) {
    const frame = await page.evaluate(
      ({ feed, after, since, checkSince }) =>
        (window as PerfWindow).__lavenderPerf?.frames.find(
          (f) => f.feed === feed && f.at > after && (!checkSince || f.fetchedAt !== since),
        ) ?? null,
      { feed, after, since: since ?? null, checkSince: newData !== undefined },
    );
    if (frame && frame.taskMs !== null) return frame;
    await page.waitForTimeout(50);
  }
  return null;
}

/**
 * Stream frames per feed since the page loaded, and how many repeated an earlier
 * `fetchedAt` (full snapshots re-sent without new data).
 */
export function streamSummary(state: PerfState) {
  const feeds = new Map<string, { frames: number; resends: number; resentBytes: number }>();
  const seen = new Set<string>();
  for (const f of state.frames) {
    const entry = feeds.get(f.feed ?? 'none') ?? { frames: 0, resends: 0, resentBytes: 0 };
    entry.frames += 1;
    const key = `${f.feed}:${f.fetchedAt}`;
    if (seen.has(key)) {
      entry.resends += 1;
      entry.resentBytes += f.bytes;
    }
    seen.add(key);
    feeds.set(f.feed ?? 'none', entry);
  }
  return Object.fromEntries(feeds);
}

/** The first rendered frame at or after `t` (page clock), or null. */
export function firstFrameAfter(state: PerfState, t: number): number | null {
  return state.frameTimes.find((f) => f >= t) ?? null;
}

export interface Layer {
  readonly name: string;
  readonly available: boolean;
  /** The catalog's availability, verbatim (why a layer cannot be turned on). */
  readonly availability: unknown;
}

/** The catalog entry for a feed: its switch label and whether it can be turned on. */
export async function layerFor(request: APIRequestContext, feed: string): Promise<Layer> {
  const response = await request.get('/api/v1/catalog');
  if (!response.ok()) throw new Error(`GET /api/v1/catalog: HTTP ${response.status()}`);
  const catalog = (await response.json()) as {
    layers: Array<{ feed: string; name: string; availability: { state: string } }>;
  };
  const layer = catalog.layers.find((l) => l.feed === feed);
  if (!layer) throw new Error(`the catalog has no ${feed} layer`);
  return {
    name: layer.name,
    available: layer.availability.state === 'available',
    availability: layer.availability,
  };
}

export interface ServerCount {
  readonly items: number;
  readonly sourceCount: number;
  readonly fetchedAt: string | null;
  readonly notices: readonly string[];
}

/** What the server currently holds for a feed (worldwide query), for object counts. */
export async function serverCount(request: APIRequestContext, feed: string): Promise<ServerCount> {
  const response = await request.get(`/api/v1/feeds/${feed}`);
  if (!response.ok()) throw new Error(`GET /api/v1/feeds/${feed}: HTTP ${response.status()}`);
  const snapshot = (await response.json()) as {
    items: { items: unknown[] };
    sourceCount: number;
    fetchedAt?: string;
    notices: string[];
  };
  return {
    items: snapshot.items.items.length,
    sourceCount: snapshot.sourceCount,
    fetchedAt: snapshot.fetchedAt ?? null,
    notices: snapshot.notices,
  };
}
