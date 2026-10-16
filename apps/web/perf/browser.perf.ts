/**
 * Browser performance scenarios (ENG-1000; targets in docs/performance/specification.md §2,
 * method §3, GEV's controls in legacy/docs/PERFORMANCE.md). Each scenario uses a fresh
 * browser context with the cache disabled and adds its section to one JSON file per run
 * (`RESULTS_FILE`), so a failed scenario does not lose the others. Unmet targets are
 * findings: nothing here asserts a target, only that each measurement actually happened
 * within its time limit.
 */
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { arch, cpus, platform, release, totalmem } from 'node:os';
import { dirname } from 'node:path';
import { type APIRequestContext, type Browser, expect, type Page, test } from '@playwright/test';
import { type WorkspaceDocument, workspaceShareUrl } from '../src/state/workspaceDocument.ts';
import { DEFAULT_COUNTS, writeAircraft } from './fixtures.ts';
import type { StreamFrame } from './instrument.ts';
import {
  firstFrameAfter,
  gpuInfo,
  heap,
  imageryMode,
  isSoftwareRenderer,
  layerFor,
  nextStreamFrame,
  openSession,
  perfState,
  restAndMotion,
  type Session,
  serverCount,
  settle,
  streamSummary,
} from './measure.ts';
import { FIXTURES, PERF_PORT, RESULTS_FILE } from './playwright.perf.config.ts';

const BASE_URL = `http://127.0.0.1:${PERF_PORT}`;
const STARTUP_RUNS = 3;
/** Keyed-update samples of 10 000 moved contacts (one per 30 s fixture refresh). */
const UPDATE_SAMPLES = 3;
/** Fixture mode re-reads its files every 30 s; one refresh plus margin. */
const REFRESH_LIMIT_MS = 40_000;

/** Merges one section into this run's results file. */
function record(section: string, value: unknown): void {
  mkdirSync(dirname(RESULTS_FILE), { recursive: true });
  const results = existsSync(RESULTS_FILE) ? JSON.parse(readFileSync(RESULTS_FILE, 'utf8')) : {};
  writeFileSync(RESULTS_FILE, `${JSON.stringify({ ...results, [section]: value }, null, 2)}\n`);
}

const round = (value: number | null | undefined) =>
  value === null || value === undefined ? null : Math.round(value * 10) / 10;

function median(values: readonly (number | null)[]): number | null {
  const sorted = values.filter((v): v is number => v !== null).sort((a, b) => a - b);
  if (sorted.length === 0) return null;
  const mid = Math.floor(sorted.length / 2);
  return round(
    sorted.length % 2 ? (sorted[mid] ?? null) : ((sorted[mid - 1] ?? 0) + (sorted[mid] ?? 0)) / 2,
  );
}

const frameCost = (frame: StreamFrame | null) =>
  frame && {
    bytes: frame.bytes,
    handlerMs: round(frame.handlerMs),
    parseMs: round(frame.parseMs),
    applyMs: round(frame.handlerMs - (frame.parseMs ?? 0)),
    taskMs: round(frame.taskMs),
  };

async function openApp(browser: Browser, path = '/'): Promise<Session> {
  const session = await openSession(browser, BASE_URL, FIXTURES);
  await session.page.goto(path);
  await expect(session.page.locator('.lv-globe-host canvas')).toHaveCount(1, { timeout: 30_000 });
  await expect(session.page.getByText('The globe could not start.', { exact: false })).toHaveCount(
    0,
  );
  return session;
}

/** How long after a stream frame arrived the globe next rendered (page clock). */
async function frameToRender(page: Page, frame: StreamFrame | null): Promise<number | null> {
  if (!frame) return null;
  await page.waitForTimeout(300);
  const rendered = firstFrameAfter(await perfState(page), frame.at + frame.handlerMs);
  return round(rendered === null ? null : rendered - frame.at);
}

/**
 * Turns a layer on (switch label from the catalog) and times click → stream frame → first
 * rendered frame after it. Returns null when the catalog says the layer is unavailable.
 */
async function activate(page: Page, request: APIRequestContext, feed: string, limitMs: number) {
  const { name, available } = await layerFor(request, feed);
  if (!available) return null;
  const start = await page.evaluate(() => performance.now());
  await page.getByRole('switch', { name, exact: true }).click();
  const frame = await nextStreamFrame(page, feed, start, limitMs);
  expect(frame, `${name}: no ${feed} snapshot within ${limitMs} ms`).not.toBeNull();
  await page.waitForTimeout(500);
  const state = await perfState(page);
  const click = state.clicks.find((c) => c.at >= start && c.label === name)?.at ?? start;
  const rendered = frame ? firstFrameAfter(state, frame.at + frame.handlerMs) : null;
  const interaction = state.events
    .filter((e) => e.start >= start && ['pointerdown', 'pointerup', 'click'].includes(e.name))
    .reduce((max, e) => Math.max(max, e.ms), 0);
  return {
    layer: name,
    /** Page clock and data time of the snapshot, to follow later refreshes of the feed. */
    snapshotAt: frame?.at ?? null,
    fetchedAt: frame?.fetchedAt ?? null,
    clickToSnapshotMs: round(frame ? frame.at - click : null),
    clickToFirstRenderMs: round(rendered === null ? null : rendered - click),
    interactionMs: interaction > 0 ? round(interaction) : '< 16 (below the Event Timing threshold)',
    snapshot: frameCost(frame),
  };
}

test.beforeAll(async ({ browser }) => {
  const gpu = await gpuInfo(browser);
  record('environment', {
    capturedAt: new Date().toISOString(),
    browser: `${browser.browserType().name()} ${browser.version()}`,
    channel: process.env.PLAYWRIGHT_CHANNEL || 'bundled chromium',
    headless: process.env.LAVENDER_PERF_HEADED !== '1',
    gpu,
    os: `${platform()} ${release()} ${arch()}`,
    cpu: cpus()[0]?.model ?? 'unknown',
    logicalCpus: cpus().length,
    memoryGiB: Math.round(totalmem() / 2 ** 30),
    viewport: '1440×900 at device pixel ratio 1',
    cache: 'fresh context per scenario, network cache disabled (CDP)',
    imagery: imageryMode(),
    fixtures: { ...DEFAULT_COUNTS, label: 'SYNTHETIC, apps/web/perf/fixtures.ts' },
  });
  if (isSoftwareRenderer(gpu.renderer) && process.env.LAVENDER_PERF_ALLOW_SOFTWARE_GL !== '1') {
    throw new Error(
      `WebGL renderer "${gpu.renderer}" is software or unavailable; frame rates would not be comparable (GEV control 1). Set LAVENDER_PERF_ALLOW_SOFTWARE_GL=1 to record anyway.`,
    );
  }
});

test.afterAll(() => console.log(`browser results: ${RESULTS_FILE}`));

/** One cold load of the default workspace (earthquakes on, default imagery). */
async function startupRun(browser: Browser, run: number) {
  const session = await openApp(browser);
  try {
    const { page, cdp } = session;
    const settledAt = await settle(page, 1_500, 15_000);
    const heapAfterSettle = await heap(cdp);
    const timing = await page.evaluate(() => {
      const resources = performance.getEntriesByType('resource') as PerformanceResourceTiming[];
      const tiles = (pattern: string) =>
        resources.filter((r) => r.name.includes(pattern)).map((r) => r.responseEnd);
      const gibs = tiles('gibs.earthdata.nasa.gov');
      const bundled = tiles('/cesium/Assets/Textures/NaturalEarthII/');
      const nav = performance.getEntriesByType('navigation')[0] as PerformanceNavigationTiming;
      return {
        firstContentfulPaintMs:
          performance.getEntriesByName('first-contentful-paint')[0]?.startTime ?? null,
        loadEventMs: nav?.loadEventEnd ?? null,
        firstTileMs: Math.min(...gibs, ...bundled),
        gibsTiles: gibs.length,
        gibsFirstTileMs: gibs.length ? Math.min(...gibs) : null,
        gibsLastTileMs: gibs.length ? Math.max(...gibs) : null,
      };
    });
    const { marks } = await perfState(page);
    return {
      run,
      shellFirstPaintMs: round(timing.firstContentfulPaintMs),
      shellInDomMs: round(marks.shell),
      globeCanvasMs: round(marks.canvas),
      engineFirstFrameMs: round(marks.firstFrame),
      firstTileMs: round(timing.firstTileMs),
      gibsFirstTileMs: round(timing.gibsFirstTileMs),
      gibsLastTileMs: round(timing.gibsLastTileMs),
      gibsTiles: timing.gibsTiles,
      settledMs: round(settledAt),
      loadEventMs: round(timing.loadEventMs),
      heapAfterSettle,
      ...(await restAndMotion(page, cdp)),
    };
  } finally {
    await session.close();
  }
}

test('startup: three cold loads of the default workspace', async ({ browser, request }) => {
  const runs: Awaited<ReturnType<typeof startupRun>>[] = [];
  for (let run = 1; run <= STARTUP_RUNS; run += 1) runs.push(await startupRun(browser, run));
  const medianOf = (pick: (r: (typeof runs)[number]) => number | null) => median(runs.map(pick));
  record('startup', {
    runs,
    median: {
      shellFirstPaintMs: medianOf((r) => r.shellFirstPaintMs),
      engineFirstFrameMs: medianOf((r) => r.engineFirstFrameMs),
      firstTileMs: medianOf((r) => r.firstTileMs),
      gibsLastTileMs: medianOf((r) => r.gibsLastTileMs),
      settledMs: medianOf((r) => r.settledMs),
      loadEventMs: medianOf((r) => r.loadEventMs),
      heapAfterSettleMiB: medianOf((r) => r.heapAfterSettle.usedMiB),
      restFrames: medianOf((r) => r.rest.frames),
      motionFps: medianOf((r) => r.motionMedianFps),
    },
    earthquakes: await serverCount(request, 'earthquakes'),
  });
});

test('aircraft: 10 000 synthetic contacts, keyed updates, then 20 000', async ({
  browser,
  request,
}) => {
  const contacts = DEFAULT_COUNTS.aircraft;
  writeAircraft(FIXTURES, contacts, 0);
  const session = await openApp(browser);
  try {
    const { page, cdp } = session;
    await settle(page, 1_500, 15_000);
    const activation = await activate(page, request, 'aircraft', REFRESH_LIMIT_MS);
    expect(activation, 'the aircraft layer is unavailable').not.toBeNull();
    // Each variant is read at the next 30 s fixture refresh: every contact moves.
    writeAircraft(FIXTURES, contacts, 1);
    // Sampling starts right after a refresh so that it ends before the next one.
    const count10k = await serverCount(request, 'aircraft');
    const at10k = await restAndMotion(page, cdp);

    // Camera moves make the client re-send its query, and the server answers with the full
    // snapshot again; only a new `fetchedAt` is a fixture refresh carrying the next variant.
    let after = activation?.snapshotAt ?? 0;
    let since = activation?.fetchedAt ?? null;
    const updates = [];
    for (let variant = 1; variant <= UPDATE_SAMPLES; variant += 1) {
      const frame = await nextStreamFrame(page, 'aircraft', after, REFRESH_LIMIT_MS, { since });
      expect(frame, `no aircraft refresh within ${REFRESH_LIMIT_MS} ms`).not.toBeNull();
      // The last refresh doubles the population: 10 000 move again, 10 000 appear.
      const next = variant + 1;
      writeAircraft(FIXTURES, next > UPDATE_SAMPLES ? 2 * contacts : contacts, next);
      updates.push({ ...frameCost(frame), frameToRenderMs: await frameToRender(page, frame) });
      after = frame?.at ?? after;
      since = frame?.fetchedAt ?? since;
    }
    // Variant UPDATE_SAMPLES + 1, written in the last pass: 10 000 moved plus 10 000 new.
    const grown = await nextStreamFrame(page, 'aircraft', after, REFRESH_LIMIT_MS, { since });
    expect(grown, 'no 20 000-contact refresh within the limit').not.toBeNull();
    const grownCost = { ...frameCost(grown), frameToRenderMs: await frameToRender(page, grown) };
    const count20k = await serverCount(request, 'aircraft');
    const at20k = await restAndMotion(page, cdp);
    record('aircraft', {
      activation: { ...activation, contacts: count10k.items },
      at10k: { server: count10k, ...at10k },
      update10kMoved: {
        samples: updates,
        medianHandlerMs: median(updates.map((u) => u.handlerMs ?? null)),
        medianApplyMs: median(updates.map((u) => u.applyMs ?? null)),
        medianTaskMs: median(updates.map((u) => u.taskMs ?? null)),
      },
      update10kMoved10kAdded: grownCost,
      at20k: { server: count20k, ...at20k },
      streams: streamSummary(await perfState(page)),
    });
  } finally {
    writeAircraft(FIXTURES, contacts, 0);
    await session.close();
  }
});

test('satellites: 5 000 synthetic element sets', async ({ browser, request }) => {
  const session = await openApp(browser);
  try {
    const { page, cdp } = session;
    await settle(page, 1_500, 15_000);
    const activation = await activate(page, request, 'satellites', REFRESH_LIMIT_MS);
    expect(activation, 'the satellites layer is unavailable').not.toBeNull();
    // Propagation runs in 1 500-satellite slices on a 1 s timer.
    await page.waitForTimeout(2_000);
    const server = await serverCount(request, 'satellites');
    const sample = await restAndMotion(page, cdp);
    record('satellites', {
      activation,
      server,
      ...sample,
      streams: streamSummary(await perfState(page)),
    });
  } finally {
    await session.close();
  }
});

test('fires: 100 000 synthetic detections, activation', async ({ browser, request }) => {
  // A scenario that cannot run is a harness failure, not a finding: stop and say why.
  const layer = await layerFor(request, 'fires');
  expect(layer.available, `${layer.name}: ${JSON.stringify(layer.availability)}`).toBe(true);
  const session = await openApp(browser);
  try {
    const { page, cdp } = session;
    await settle(page, 1_500, 15_000);
    const activation = await activate(page, request, 'fires', 60_000);
    expect(activation, 'the fires layer is unavailable').not.toBeNull();
    await settle(page, 1_500, 10_000);
    record('fires', {
      activation,
      server: await serverCount(request, 'fires'),
      ...(await restAndMotion(page, cdp)),
    });
  } finally {
    await session.close();
  }
});

test('five layers on: aircraft, satellites, earthquakes, fires, cyclones', async ({
  browser,
  request,
}) => {
  const wanted = ['aircraft', 'satellites', 'earthquakes', 'fires', 'cyclones'];
  const layers = await Promise.all(wanted.map((feed) => layerFor(request, feed)));
  expect(
    layers.filter((l) => !l.available),
    'layers the catalog does not offer',
  ).toEqual([]);
  const feeds = wanted;
  const workspace: WorkspaceDocument = {
    schemaVersion: 2,
    camera: {
      lon: 10,
      lat: 20,
      heightM: 20_000_000,
      headingRad: 0,
      pitchRad: -Math.PI / 2,
      rollRad: 0,
    },
    layers: feeds.map((id) => ({ id, enabled: true, options: {} })),
    imageryId: null,
    selection: null,
    annotations: [],
  };
  const session = await openApp(browser, workspaceShareUrl(workspace, `${BASE_URL}/`));
  try {
    const { page, cdp } = session;
    const arrived = await Promise.all(feeds.map((feed) => nextStreamFrame(page, feed, 0, 60_000)));
    expect(
      arrived.every(Boolean),
      `missing snapshots: ${feeds.filter((_, i) => !arrived[i])}`,
    ).toBe(true);
    await page.waitForTimeout(2_000);
    const counts = Object.fromEntries(
      await Promise.all(
        feeds.map(async (feed) => [feed, await serverCount(request, feed)] as const),
      ),
    );
    const sample = await restAndMotion(page, cdp);
    record('combined', {
      feeds,
      counts,
      ...sample,
      streams: streamSummary(await perfState(page)),
    });
  } finally {
    await session.close();
  }
});
