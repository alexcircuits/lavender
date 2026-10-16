/**
 * ENG-980 lane evidence (not the ENG-1000 harness): frame rate with 12 000 synthetic vessels
 * while the map moves and at rest, and the main-thread cost of a 12 000-vessel update.
 * Opt-in, because it measures the machine it runs on:
 *
 *   LAVENDER_E2E_PERF=1 LAVENDER_E2E_EVIDENCE_DIR=<dir outside the repo> \
 *     PLAYWRIGHT_CHANNEL=chrome pnpm run e2e vessels-perf.spec.ts
 *
 * Method (docs/performance/specification.md §1, §3, GEV's capture controls): 1440×900 at
 * DPR 1, the production build from the real server, the vessels stream answered locally with
 * a deterministic SYNTHETIC picture (e2e/syntheticVessels.ts, every streamed field filled in)
 * through Playwright's WebSocket routing, so the browser runs the real WebPlatform →
 * FeedController → VesselRenderer path. Every repetition starts from a fresh load of the
 * scenario's view, so no drift or inertia from an earlier run moves the ships out of view.
 * Motion is a scripted pointer drag of two whole circles (the globe turns under it and comes
 * back) that moves the camera on every animation frame for 5 s; rest is 5 s with no input,
 * once the page has drawn nothing for 1 s after the drag (moveEnd, the view update and the
 * snapshot it brings, late tiles; the time that took is recorded). A frame counts as
 * rendered when the page issued WebGL draw calls in it (counted by wrapping the draw
 * functions), so request-render mode shows as 0 rendered frames at rest. Three
 * repetitions per scenario; medians reported, and the spec fails below 45 fps in motion or
 * with any frame drawn at rest. The display's 60 Hz caps that figure, so a second run of the
 * dense scenario starts its own Chrome with vsync and the frame-rate limit off to show the
 * headroom (GPU work included, as frames cannot outrun the GPU). The GPU renderer string is
 * recorded. Update cost is the time spent in the stream's message handler (JSON parse, feed
 * controller, keyed diff of every billboard, store update), with the parse alone measured
 * separately, over ten updates that move every ship under way; the spec fails when the
 * apply (handler minus parse) exceeds the 8 ms budget for a feed update (specification §2).
 * The main-thread work of the frame that draws each update (billboard buffer writes) is
 * recorded as well.
 */
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import {
  type APIRequestContext,
  type Browser,
  expect,
  type Page,
  type TestInfo,
  test,
} from '@playwright/test';
import type { ClientFrame, Vessel } from '../src/contracts';
import { thinnedOut, thinningCellDeg, vesselSymbol } from '../src/globe/renderers/vesselSymbology';
import { type WorkspaceDocument, workspaceShareUrl } from '../src/state/workspaceDocument';
import { availableLayers, GLOBE_CANVAS } from './support';
import {
  advanced,
  type Box,
  NORTH_SEA,
  syntheticSnapshot,
  syntheticVessels,
  WORLD_SEAS,
} from './syntheticVessels';

const VESSELS = 12_000;
const MOTION_MS = 5_000;
const REST_MS = 5_000;
const REPEATS = 3;
const UPDATES = 10;
/** The Done-when floor (ENG-980): 12 000 ships at 45 fps or more while the map moves. */
const MIN_MOTION_FPS = 45;
/** Feed update apply budget, main thread (docs/performance/specification.md §2). */
const MAX_APPLY_MS = 8;
const EVIDENCE_DIR = process.env.LAVENDER_E2E_EVIDENCE_DIR;
const VIEWPORT = { width: 1440, height: 900 };
/** Chrome switches that stop rAF waiting for the 60 Hz display (headroom run only). */
const UNCAPPED_ARGS = ['--disable-gpu-vsync', '--disable-frame-rate-limit'];

test.use({ viewport: VIEWPORT, deviceScaleFactor: 1 });

interface PerfWindow {
  __lvPerf: {
    draws: number;
    /** Milliseconds spent inside requestAnimationFrame callbacks (Cesium's render loop). */
    frameWork: number;
    messages: { ms: number; parseMs: number; bytes: number }[];
    /** Work of the first drawing frame after each stream message (its buffer writes). */
    updateFrames: number[];
    awaitingUpdateFrame: boolean;
  };
}

/** Counts WebGL draw calls and times the stream's message handler. Runs before the app. */
function instrument() {
  const perf: PerfWindow['__lvPerf'] = {
    draws: 0,
    frameWork: 0,
    messages: [],
    updateFrames: [],
    awaitingUpdateFrame: false,
  };
  (window as unknown as PerfWindow).__lvPerf = perf;
  // Main-thread work per frame: every animation-frame callback is timed (update, render
  // commands and the pointer handling of the drag; GPU time is not included).
  const raf = window.requestAnimationFrame.bind(window);
  window.requestAnimationFrame = (callback) =>
    raf((time) => {
      const started = performance.now();
      const draws = perf.draws;
      try {
        callback(time);
      } finally {
        const ms = performance.now() - started;
        perf.frameWork += ms;
        if (perf.awaitingUpdateFrame && perf.draws !== draws) {
          perf.updateFrames.push(ms);
          perf.awaitingUpdateFrame = false;
        }
      }
    });
  const names = [
    'drawArrays',
    'drawElements',
    'drawArraysInstanced',
    'drawElementsInstanced',
    'drawRangeElements',
  ] as const;
  for (const proto of [WebGL2RenderingContext.prototype, WebGLRenderingContext.prototype]) {
    for (const name of names) {
      const target = proto as unknown as Record<string, unknown>;
      const original = target[name];
      if (typeof original !== 'function') continue;
      target[name] = function (this: unknown, ...args: unknown[]) {
        perf.draws += 1;
        return (original as (...a: unknown[]) => unknown).apply(this, args);
      };
    }
  }
  // Playwright's WebSocket routing replaces the page's WebSocket class, so the stream
  // handler is timed through the one call it always makes: JSON.parse of the frame. A
  // microtask queued at parse time runs when the synchronous handler (feed controller, keyed
  // diff, store update) has returned. Only stream-sized payloads are timed.
  const parse = JSON.parse;
  JSON.parse = function (
    this: unknown,
    text: string,
    reviver?: (k: string, v: unknown) => unknown,
  ) {
    if (typeof text !== 'string' || text.length < 100_000) return parse.call(JSON, text, reviver);
    const started = performance.now();
    const value = parse.call(JSON, text, reviver);
    const parsed = performance.now();
    queueMicrotask(() => {
      perf.messages.push({
        ms: performance.now() - started,
        parseMs: parsed - started,
        bytes: text.length,
      });
      perf.awaitingUpdateFrame = true;
    });
    return value;
  } as typeof JSON.parse;
}

interface Sample {
  readonly durationMs: number;
  readonly animationFrames: number;
  readonly renderedFrames: number;
  readonly renderedFps: number;
  readonly medianFrameMs: number;
  readonly p95FrameMs: number;
  /** Main-thread animation-frame work in rendered frames (headroom below the frame time). */
  readonly medianWorkMs: number;
  readonly p95WorkMs: number;
}

/**
 * Samples animation frames for `durationMs`; with `drag`, moves the pointer in a circle on
 * the globe every frame (a left-drag: Cesium spins the globe under it, so every frame moves
 * the camera and must render).
 */
async function sample(page: Page, durationMs: number, drag: boolean): Promise<Sample> {
  return page.evaluate(
    ({ durationMs, drag, selector }) =>
      new Promise<Sample>((resolve) => {
        const perf = (window as unknown as PerfWindow).__lvPerf;
        const canvas = document.querySelector(selector) as HTMLCanvasElement;
        const box = canvas.getBoundingClientRect();
        const cx = box.left + box.width / 2;
        const cy = box.top + box.height / 2;
        const radius = Math.min(box.width, box.height) / 8;
        const pointer = (type: string, x: number, y: number) =>
          canvas.dispatchEvent(
            new PointerEvent(type, {
              bubbles: true,
              cancelable: true,
              clientX: x,
              clientY: y,
              pointerId: 1,
              pointerType: 'mouse',
              isPrimary: true,
              button: 0,
              buttons: type === 'pointerup' ? 0 : 1,
            }),
          );
        const stamps: number[] = [];
        const drew: boolean[] = [];
        const work: number[] = [];
        let draws = perf.draws;
        let frameWork = perf.frameWork;
        let start = -1;
        let at = { x: cx + radius, y: cy };
        const tick = (now: number) => {
          if (start < 0) {
            start = now;
            if (drag) pointer('pointerdown', at.x, at.y);
          } else {
            stamps.push(now);
            drew.push(perf.draws !== draws);
            work.push(perf.frameWork - frameWork);
          }
          draws = perf.draws;
          frameWork = perf.frameWork;
          const elapsed = now - start;
          if (elapsed < durationMs) {
            if (drag) {
              // Two whole circles: the globe turns under the pointer and comes back, so the
              // ships stay in view.
              const angle = (4 * Math.PI * elapsed) / durationMs;
              at = { x: cx + radius * Math.cos(angle), y: cy + radius * Math.sin(angle) };
              pointer('pointermove', at.x, at.y);
            }
            requestAnimationFrame(tick);
            return;
          }
          if (drag) {
            // Two whole circles end where the press began, which would read as a click and
            // select a ship: release 24 px away (beyond Cesium's 5 px click tolerance).
            const release = { x: cx + radius - 24, y: cy };
            pointer('pointermove', release.x, release.y);
            pointer('pointerup', release.x, release.y);
          }
          const quantile = (values: number[], q: number) => {
            const sorted = [...values].sort((a, b) => a - b);
            return sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))] ?? 0;
          };
          const intervals = stamps.slice(1).map((t, i) => t - (stamps[i] ?? t));
          const rendered = intervals.filter((_, i) => drew[i + 1]);
          const renderedWork = work.filter((_, i) => drew[i]);
          const renderedFrames = drew.filter(Boolean).length;
          resolve({
            durationMs: elapsed,
            animationFrames: stamps.length,
            renderedFrames,
            renderedFps: (renderedFrames * 1000) / elapsed,
            medianFrameMs: quantile(rendered, 0.5),
            p95FrameMs: quantile(rendered, 0.95),
            medianWorkMs: quantile(renderedWork, 0.5),
            p95WorkMs: quantile(renderedWork, 0.95),
          });
        };
        requestAnimationFrame(tick);
      }),
    { durationMs, drag, selector: GLOBE_CANVAS },
  );
}

/**
 * Waits until the page has drawn nothing for `quietMs` (the drag's moveEnd, the debounced
 * view update and the snapshot it brings, late imagery tiles), at most `maxMs`; returns how
 * long that took. The rest window then shows whether anything keeps redrawing.
 */
async function settle(page: Page, quietMs = 1_000, maxMs = 20_000): Promise<number> {
  return page.evaluate(
    ({ quietMs, maxMs }) =>
      new Promise<number>((resolve) => {
        const perf = (window as unknown as PerfWindow).__lvPerf;
        const started = performance.now();
        let draws = perf.draws;
        let lastDraw = started;
        const tick = (now: number) => {
          if (perf.draws !== draws) {
            draws = perf.draws;
            lastDraw = now;
          }
          if (now - lastDraw >= quietMs || now - started >= maxMs) resolve(now - started);
          else requestAnimationFrame(tick);
        };
        requestAnimationFrame(tick);
      }),
    { quietMs, maxMs },
  );
}

const median = (values: number[]) => {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)] ?? 0;
};

function evidence(testInfo: TestInfo, name: string, body: unknown) {
  const text = JSON.stringify(body, null, 2);
  if (EVIDENCE_DIR) {
    mkdirSync(EVIDENCE_DIR, { recursive: true });
    writeFileSync(join(EVIDENCE_DIR, name), text);
  }
  return testInfo.attach(name, { body: text, contentType: 'application/json' });
}

/** Ships the renderer draws at this camera height (the same thinning rule it uses). */
function drawnAt(vessels: readonly Vessel[], heightM: number): number {
  const cell = thinningCellDeg(heightM);
  const hidden = thinnedOut(
    vessels.map(
      (v) =>
        [
          v.mmsi,
          {
            lat: v.position.lat,
            lon: v.position.lon,
            underway: vesselSymbol(v).kind === 'underway',
          },
        ] as const,
    ),
    cell,
    null,
  );
  return vessels.length - hidden.size;
}

interface Scenario {
  readonly name: string;
  readonly boxes: readonly Box[];
  readonly camera: { lon: number; lat: number; heightM: number };
}

const SCENARIOS: readonly Scenario[] = [
  // All 12 000 inside one view below the thinning height: every ship is drawn.
  {
    name: 'north-sea-all-drawn',
    boxes: NORTH_SEA,
    camera: { lon: 4, lat: 54.5, heightM: 900_000 },
  },
  // The whole globe from the home height: low-zoom thinning applies.
  { name: 'world-thinned', boxes: WORLD_SEAS, camera: { lon: 10, lat: 20, heightM: 20_000_000 } },
];

interface Stream {
  ships: Vessel[];
  socket: { send(message: string): void } | null;
  subscription: string | null;
}

/** Answers the page's vessels subscription with the synthetic picture; instruments the page. */
async function prepare(page: Page, ships: Vessel[]): Promise<Stream> {
  const stream: Stream = { ships, socket: null, subscription: null };
  await page.routeWebSocket(/\/api\/v1\/stream$/, (ws) => {
    stream.socket = ws;
    ws.onMessage((message) => {
      const frame = JSON.parse(String(message)) as ClientFrame;
      if (frame.op === 'unsubscribe' || frame.query.feed !== 'vessels') return;
      stream.subscription = frame.id;
      ws.send(
        JSON.stringify({
          type: 'snapshot',
          id: frame.id,
          snapshot: syntheticSnapshot(stream.ships),
        }),
      );
    });
  });
  await page.addInitScript(instrument);
  return stream;
}

/** A fresh load of the scenario's view with every ship delivered and the first frames done. */
async function load(
  page: Page,
  request: APIRequestContext,
  baseURL: string | undefined,
  scenario: Scenario,
) {
  const { catalog } = await availableLayers(request, ['vessels']);
  const doc: WorkspaceDocument = {
    schemaVersion: 2,
    camera: { ...scenario.camera, headingRad: 0, pitchRad: -Math.PI / 2, rollRad: 0 },
    layers: catalog.layers.map(({ feed }) => ({
      id: feed,
      enabled: feed === 'vessels',
      options: {},
    })),
    imageryId: 'natural-earth-ii',
    selection: null,
    annotations: [],
  };
  await page.goto('about:blank');
  await page.goto(workspaceShareUrl(doc, baseURL ?? 'http://127.0.0.1:7412/'));
  await expect(page.getByText('Loading globe…')).toBeHidden({ timeout: 60_000 });
  await expect(page.locator('[data-feed="vessels"]')).toContainText(/12\s?000/, {
    timeout: 60_000,
  });
  await page.waitForTimeout(3_000); // first upload, imagery and the view update settle
}

async function gpuOf(page: Page) {
  return page.evaluate(() => {
    const gl = document.createElement('canvas').getContext('webgl2');
    const info = gl?.getExtension('WEBGL_debug_renderer_info');
    return {
      renderer: info ? String(gl?.getParameter(info.UNMASKED_RENDERER_WEBGL)) : 'unknown',
      vendor: info ? String(gl?.getParameter(info.UNMASKED_VENDOR_WEBGL)) : 'unknown',
    };
  });
}

function shotPath(testInfo: TestInfo, name: string): string {
  if (!EVIDENCE_DIR) return testInfo.outputPath(name);
  mkdirSync(EVIDENCE_DIR, { recursive: true });
  return join(EVIDENCE_DIR, name);
}

function motionSummary(motion: Sample[]) {
  return {
    runs: motion,
    medianRenderedFps: median(motion.map((s) => s.renderedFps)),
    medianFrameMs: median(motion.map((s) => s.medianFrameMs)),
    medianP95FrameMs: median(motion.map((s) => s.p95FrameMs)),
    medianWorkMs: median(motion.map((s) => s.medianWorkMs)),
    medianP95WorkMs: median(motion.map((s) => s.p95WorkMs)),
  };
}

function describeRun(browser: Browser, scenario: Scenario) {
  return {
    task: 'ENG-980 lane evidence (not the ENG-1000 harness)',
    fixture: `SYNTHETIC: ${VESSELS} invented vessels (e2e/syntheticVessels.ts, seed 980)`,
    scenario: scenario.name,
    camera: scenario.camera,
    drawnAfterThinning: drawnAt(syntheticVessels(VESSELS, scenario.boxes), scenario.camera.heightM),
    browser: `${browser.browserType().name()} ${browser.version()}`,
    headless: true,
    viewport: '1440x900 @1x',
    gevBaseline:
      'GEV: 22 fps in motion with 12 000 AIS vessels (docs/performance/specification.md)',
    target: '≥ 45 fps in motion; 0 frames at rest; update apply (10 000 diffs) ≤ 8 ms',
  };
}

for (const scenario of SCENARIOS) {
  test(`12 000 synthetic vessels: frame rate in motion and at rest (${scenario.name})`, async ({
    page,
    request,
    baseURL,
    browser,
  }, testInfo) => {
    test.skip(process.env.LAVENDER_E2E_PERF !== '1', 'Opt-in measurement: LAVENDER_E2E_PERF=1.');
    test.setTimeout(300_000);
    const stream = await prepare(page, syntheticVessels(VESSELS, scenario.boxes));

    const motion: Sample[] = [];
    const rest: Sample[] = [];
    const settledMs: number[] = [];
    for (let run = 0; run < REPEATS; run += 1) {
      await load(page, request, baseURL, scenario);
      motion.push(await sample(page, MOTION_MS, true));
      // The drag must not have picked a ship (an open inspector would skew the rest window).
      await expect(page.locator('[data-inspector]')).toHaveCount(0);
      if (run === REPEATS - 1) {
        // Where the drag left the camera: the ships must still fill the view.
        await page.screenshot({
          path: shotPath(testInfo, `perf-${scenario.name}-after-motion.png`),
        });
      }
      settledMs.push(await settle(page));
      rest.push(await sample(page, REST_MS, false));
    }
    const gpu = await gpuOf(page);

    // Ten updates that move every ship under way, one per second, as a busy stream would.
    const before = await page.evaluate(() => {
      const perf = (window as unknown as PerfWindow).__lvPerf;
      perf.updateFrames = [];
      perf.awaitingUpdateFrame = false;
      return perf.messages.length;
    });
    let moved = 0;
    for (let i = 0; i < UPDATES; i += 1) {
      const previous = stream.ships;
      stream.ships = advanced(stream.ships, 10);
      moved = stream.ships.filter((ship, k) => ship !== previous[k]).length;
      const sent = JSON.stringify({
        type: 'snapshot',
        id: stream.subscription,
        snapshot: syntheticSnapshot(stream.ships),
      });
      stream.socket?.send(sent);
      await page.waitForTimeout(1_000);
    }
    const updates = await page.evaluate((from) => {
      const perf = (window as unknown as PerfWindow).__lvPerf;
      return { handler: perf.messages.slice(from), frames: perf.updateFrames };
    }, before);
    // The live heap after a full collection (specification §3: CDP), not whatever garbage
    // the last updates left behind.
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('HeapProfiler.collectGarbage');
    const heap = (await cdp.send('Runtime.getHeapUsage')).usedSize;
    await cdp.detach();
    await page.screenshot({ path: shotPath(testInfo, `perf-${scenario.name}.png`) });

    const handlerMs = updates.handler.map((m) => m.ms);
    const parseMs = updates.handler.map((m) => m.parseMs);
    const afterParseMs = updates.handler.map((m) => m.ms - m.parseMs);
    const result = {
      ...describeRun(browser, scenario),
      gpu,
      motion: motionSummary(motion),
      rest: {
        settledAfterMotionMs: settledMs,
        runs: rest,
        medianRenderedFrames: median(rest.map((s) => s.renderedFrames)),
      },
      update: {
        snapshots: updates.handler.length,
        shipsPerSnapshot: VESSELS,
        movedPerSnapshot: moved,
        bytes: updates.handler[0]?.bytes ?? 0,
        handlerMs,
        medianHandlerMs: median(handlerMs),
        maxHandlerMs: handlerMs.reduce((max, ms) => Math.max(max, ms), 0),
        medianJsonParseMs: median(parseMs),
        afterParseMs,
        medianAfterParseMs: median(afterParseMs),
        applyBudgetMs: MAX_APPLY_MS,
        withinApplyBudget: median(afterParseMs) <= MAX_APPLY_MS,
        drawingFrameMs: updates.frames,
        medianDrawingFrameMs: median(updates.frames),
        note: 'handler = JSON parse + FeedController + keyed diff of every ship + store update; afterParse = handler minus parse (the apply the 8 ms budget covers: diff, billboard writes, thinning, labels, summary); drawingFrame = main-thread work of the next frame that draws (billboard buffer writes and render commands, not GPU time)',
      },
      usedJSHeapBytesAfterGc: heap,
    };
    await evidence(testInfo, `perf-${scenario.name}.json`, result);
    expect(updates.handler.length).toBe(UPDATES);
    expect(result.motion.medianRenderedFps).toBeGreaterThanOrEqual(MIN_MOTION_FPS);
    expect(rest.map((s) => s.renderedFrames)).toEqual(rest.map(() => 0));
    expect(result.update.medianAfterParseMs).toBeLessThanOrEqual(MAX_APPLY_MS);
  });
}

test('12 000 synthetic vessels: headroom with the 60 Hz cap off (north-sea-all-drawn)', async ({
  playwright,
  request,
  baseURL,
}, testInfo) => {
  test.skip(process.env.LAVENDER_E2E_PERF !== '1', 'Opt-in measurement: LAVENDER_E2E_PERF=1.');
  test.setTimeout(300_000);
  const scenario = SCENARIOS[0];
  if (!scenario) throw new Error('no scenario');
  const channel = process.env.PLAYWRIGHT_CHANNEL;
  const browser = await playwright.chromium.launch({
    ...(channel ? { channel } : {}),
    args: UNCAPPED_ARGS,
  });
  try {
    const context = await browser.newContext({
      viewport: VIEWPORT,
      deviceScaleFactor: 1,
      ...(baseURL ? { baseURL } : {}),
    });
    const page = await context.newPage();
    await prepare(page, syntheticVessels(VESSELS, scenario.boxes));
    const motion: Sample[] = [];
    for (let run = 0; run < REPEATS; run += 1) {
      await load(page, request, baseURL, scenario);
      motion.push(await sample(page, MOTION_MS, true));
      await expect(page.locator('[data-inspector]')).toHaveCount(0);
    }
    await page.screenshot({ path: shotPath(testInfo, 'perf-uncapped-after-motion.png') });
    const summary = motionSummary(motion);
    await evidence(testInfo, 'perf-uncapped-north-sea.json', {
      ...describeRun(browser, scenario),
      chromeArgs: UNCAPPED_ARGS,
      gpu: await gpuOf(page),
      motion: summary,
      // Above 60 fps only if the switches took effect; otherwise this run is capped too.
      uncapped: summary.medianRenderedFps > 65,
    });
    expect(summary.medianRenderedFps).toBeGreaterThanOrEqual(MIN_MOTION_FPS);
  } finally {
    await browser.close();
  }
});
