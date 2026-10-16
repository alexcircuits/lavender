/**
 * ENG-975 acceptance: earthquake, fire and cyclone symbols are drawn, legible and keyed by a
 * legend that matches the hazard ramp over BOTH Blue Marble and Black Marble base imagery.
 *
 * The feeds are the recorded fixtures replayed by the real lavender-server. By default NASA
 * GIBS tiles are stubbed (hermetic CI run: the symbols, legends and imagery switch are still
 * checked). For the visual review run against real tiles:
 *
 *   LAVENDER_E2E_LIVE_IMAGERY=1 LAVENDER_E2E_EVIDENCE_DIR=<dir outside the repo> \
 *     PLAYWRIGHT_CHANNEL=chrome pnpm run e2e hazard-imagery.spec.ts
 *
 * Screenshots and the timing JSON go to LAVENDER_E2E_EVIDENCE_DIR when set (else the test's
 * output directory). Nothing here is live hazard data.
 */
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { performance } from 'node:perf_hooks';
import { expect, type Page, type TestInfo, test } from '@playwright/test';
import { type WorkspaceDocument, workspaceShareUrl } from '../src/state/workspaceDocument';
import {
  availableLayers,
  GLOBE_CANVAS,
  gibsTilesSettled,
  HAZARD_RAMP,
  rampPixels,
  recordGibsTiles,
  scanRamp,
  type TileResponse,
  watchPage,
} from './support';

const LIVE_IMAGERY = process.env.LAVENDER_E2E_LIVE_IMAGERY === '1';
const EVIDENCE_DIR = process.env.LAVENDER_E2E_EVIDENCE_DIR;

const IMAGERY = [
  {
    id: 'gibs-blue-marble',
    name: 'Relief and bathymetry (Blue Marble)',
    layer: 'BlueMarble_ShadedRelief_Bathymetry',
  },
  {
    id: 'gibs-black-marble',
    name: 'Night lights (VIIRS Black Marble 2016)',
    layer: 'VIIRS_Black_Marble',
  },
] as const;

type Hazard = 'earthquakes' | 'fires' | 'cyclones';
const HAZARD_FEEDS: readonly Hazard[] = ['earthquakes', 'fires', 'cyclones'];
/** Layer names are the server catalog's (read before each test), not this file's. */
let HAZARDS: Readonly<Record<Hazard, string>> = { earthquakes: '', fires: '', cyclones: '' };

test.beforeEach(async ({ request }) => {
  const { layers } = await availableLayers(request, HAZARD_FEEDS);
  HAZARDS = {
    earthquakes: layers.earthquakes.name,
    fires: layers.fires.name,
    cyclones: layers.cyclones.name,
  };
});

const pose = (lon: number, lat: number, heightM: number) => ({
  lon,
  lat,
  heightM,
  headingRad: 0,
  pitchRad: -Math.PI / 2,
  rollRad: 0,
});

/**
 * The ramp steps each hazard alone must add in a view: drawn per hazard, so a renderer that
 * draws nothing cannot hide behind another layer that shares the ramp.
 */
type Expected = Partial<Record<Hazard, readonly number[]>>;

/** TEST ALPHA (20° N 60° W) with its track and cone, and the Puerto Rico / Panama quakes. */
const CARIBBEAN = pose(-64, 18, 3_500_000);
const CARIBBEAN_STEPS: Expected = {
  earthquakes: [0, 1, 2, 3], // M2.85–5.0
  cyclones: [2], // 80 kn position, track and cone outline
};
/** The eight synthetic FIRMS detections, which span all five FRP bins. */
const HELSINKI = pose(24.94, 60.3, 400_000);
const HELSINKI_STEPS: Expected = { fires: [0, 1, 2, 3, 4] };
/** The Atlantic between the Caribbean and Finland: every fixture hazard at once. */
const OVERVIEW = pose(-25, 40, 16_000_000);
const OVERVIEW_STEPS: Expected = {
  earthquakes: [0, 1, 2, 3],
  // Seven 1° cells that overlap at this height; the 60 MW cell (step 2) lies under its larger
  // neighbours. Helsinki above checks every fire step.
  fires: [0, 1, 3, 4],
  cyclones: [2],
};

/** Legends as the layer options show them, the symbol shape and the ramp step of each swatch. */
const LEGENDS: readonly {
  readonly hazard: Hazard;
  readonly name: string;
  readonly labels: string[];
  readonly shape: 'ring' | 'cell' | 'spiral';
}[] = [
  {
    hazard: 'earthquakes',
    name: 'Earthquake magnitude legend',
    labels: ['<M3', 'M3–<4', 'M4–<5', 'M5–<6', '≥M6'],
    shape: 'ring',
  },
  {
    hazard: 'fires',
    name: 'Active fire intensity legend',
    labels: ['<10 MW', '10–<50 MW', '50–<100 MW', '100–<250 MW', '≥250 MW'],
    shape: 'cell',
  },
  {
    hazard: 'cyclones',
    name: 'Cyclone sustained-wind legend',
    labels: ['<34 kn', '34–63 kn', '64–95 kn', '96–136 kn', '≥137 kn', 'Wind not reported'],
    shape: 'spiral',
  },
];

function document(camera: ReturnType<typeof pose>, imageryId: string): WorkspaceDocument {
  return {
    schemaVersion: 2,
    camera,
    layers: HAZARD_FEEDS.map((id) => ({ id, enabled: false, options: {} })),
    imageryId,
    selection: null,
    annotations: [],
  };
}

/** A fresh load (a hash-only change would not reload the app) with every hazard layer off. */
async function open(page: Page, baseURL: string | undefined, doc: WorkspaceDocument) {
  await page.goto('about:blank');
  await page.goto(workspaceShareUrl(doc, baseURL ?? 'http://127.0.0.1:7412/'));
  await expect(page.getByText('Loading globe…')).toBeHidden({ timeout: 30_000 });
  for (const name of Object.values(HAZARDS)) {
    await expect(page.getByRole('switch', { name, exact: true })).not.toBeChecked();
  }
}

/** Real tiles only: wait until the imagery has loaded and stopped changing. */
async function imageryReady(page: Page, tiles: readonly TileResponse[], layer: string) {
  if (LIVE_IMAGERY) await gibsTilesSettled(page, tiles, layer);
}

/**
 * The light keyline of the two-tone hazard edge (`--map-keyline`, docs/design/design-system.md
 * §4): what keeps the darkest ramp steps at 3:1 over dark imagery. Each hazard must add at
 * least this many keyline pixels to a view (its glyphs and cyclone lines draw it).
 */
const KEYLINE = '#bcbcc4';
const MIN_KEYLINE_PIXELS = 20;
const keylinePixels = async (page: Page) =>
  (await scanRamp(page, undefined, 10, [KEYLINE])).counts[0] ?? 0;

/** Per ramp step, the pixels a hazard added to a view, and its keyline pixels. */
interface HazardGain {
  readonly steps: number[];
  readonly keyline: number;
}

const subtract = (after: readonly number[], before: readonly number[]) =>
  after.map((count, step) => count - (before[step] ?? 0));
const maxOf = (a: readonly number[], b: readonly number[]) =>
  a.map((count, step) => Math.max(count, b[step] ?? 0));
/** Per expected step, whether it gained pixels; a failing poll shows which step is missing. */
const drawnSteps = (gain: readonly number[], steps: readonly number[]) =>
  steps.map((step) => `step ${step}: ${(gain[step] ?? 0) > 0 ? 'drawn' : 'missing'}`);
const allDrawn = (steps: readonly number[]) => steps.map((step) => `step ${step}: drawn`);

async function setHazards(page: Page, hazards: readonly Hazard[], on: boolean) {
  for (const hazard of hazards) {
    const toggle = page.getByRole('switch', { name: HAZARDS[hazard], exact: true });
    if ((await toggle.isChecked()) !== on) await toggle.click();
  }
}

/**
 * Turns one hazard on by itself and returns, per ramp step, how many more pixels carry that
 * colour than with it off (measured before and after, so imagery that refines meanwhile does
 * not count as a symbol), and likewise its keyline pixels. Fails unless every one of `steps`
 * gained pixels and the hazard edge's keyline is drawn. Leaves it off.
 */
async function hazardGain(
  page: Page,
  hazard: Hazard,
  steps: readonly number[],
): Promise<HazardGain> {
  const before = await rampPixels(page);
  const keylineBefore = await keylinePixels(page);
  await setHazards(page, [hazard], true);
  await expect
    .poll(async () => drawnSteps(subtract(await rampPixels(page), before), steps), {
      message: `${hazard} alone draws its ramp steps`,
      timeout: 15_000,
    })
    .toEqual(allDrawn(steps));
  await page.waitForTimeout(300); // let a following frame (forecast cone) settle
  const on = await rampPixels(page);
  const keylineOn = await keylinePixels(page);
  await setHazards(page, [hazard], false);
  await expect
    .poll(async () => subtract(await rampPixels(page), before).every((n) => n <= 50), {
      message: `${hazard} symbols cleared`,
    })
    .toBe(true);
  const gain = subtract(on, maxOf(before, await rampPixels(page)));
  expect(drawnSteps(gain, steps), `${hazard}: ${gain.join(', ')}`).toEqual(allDrawn(steps));
  const keyline = keylineOn - Math.max(keylineBefore, await keylinePixels(page));
  expect(keyline, `${hazard}: keyline pixels of the hazard edge`).toBeGreaterThanOrEqual(
    MIN_KEYLINE_PIXELS,
  );
  return { steps: gain, keyline };
}

/**
 * Each hazard expected in the view by itself, then every hazard layer on together for the
 * screenshot (left on for the legends).
 */
async function checkView(
  page: Page,
  expected: Expected,
  capture: () => Promise<void>,
): Promise<Partial<Record<Hazard, HazardGain>>> {
  const hazards = HAZARD_FEEDS.filter((hazard) => expected[hazard]);
  const gains: Partial<Record<Hazard, HazardGain>> = {};
  for (const hazard of hazards) {
    gains[hazard] = await hazardGain(page, hazard, expected[hazard] ?? []);
  }
  const before = await rampPixels(page);
  const all = [...new Set(hazards.flatMap((hazard) => expected[hazard] ?? []))];
  await setHazards(page, HAZARD_FEEDS, true);
  await expect
    .poll(async () => drawnSteps(subtract(await rampPixels(page), before), all), {
      message: 'every hazard together draws the same ramp steps',
      timeout: 15_000,
    })
    .toEqual(allDrawn(all));
  await page.waitForTimeout(300); // the forecast cone and halo frames
  await capture();
  return gains;
}

function evidencePath(testInfo: TestInfo, name: string): string {
  if (!EVIDENCE_DIR) return testInfo.outputPath(name);
  mkdirSync(EVIDENCE_DIR, { recursive: true });
  return join(EVIDENCE_DIR, name);
}

/**
 * The page at CSS size with the Layers panel closed (it floats over the globe), plus a
 * device-resolution close-up of the middle of the globe, where each view centres its symbols.
 */
async function screenshot(page: Page, testInfo: TestInfo, name: string) {
  const panel = page.getByRole('region', { name: 'Layers', exact: true });
  await page.getByRole('button', { name: 'Close layers', exact: true }).click();
  await expect(panel).toBeHidden();
  await page.screenshot({ path: evidencePath(testInfo, `${name}.png`), scale: 'css' });
  const box = await page.locator(GLOBE_CANVAS).boundingBox();
  if (box) {
    await page.screenshot({
      path: evidencePath(testInfo, `${name}-closeup.png`),
      clip: {
        x: box.x + box.width / 2 - 240,
        y: box.y + box.height / 2 - 160,
        width: 480,
        height: 320,
      },
      scale: 'device',
    });
  }
  await page.getByRole('button', { name: 'Layers', exact: true }).click();
  await expect(panel).toBeVisible();
}

const escaped = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

const rgb = (hex: string) =>
  `rgb(${[1, 3, 5].map((at) => Number.parseInt(hex.slice(at, at + 2), 16)).join(', ')})`;

async function expectLegendsMatchRamp(page: Page, testInfo: TestInfo, tag: string) {
  for (const legend of LEGENDS) {
    const options = page.getByRole('button', { name: `${HAZARDS[legend.hazard]} options` });
    if ((await options.getAttribute('aria-expanded')) !== 'true') await options.click();
    const list = page.getByRole('list', { name: legend.name });
    await expect(list).toBeVisible();
    await expect(list.getByRole('listitem')).toHaveText(legend.labels);
    const swatches = await list
      .locator('[data-step]')
      .evaluateAll((elements) =>
        elements.map((el) => [
          el.getAttribute('data-step'),
          getComputedStyle(el).color,
          el.getAttribute('data-shape'),
        ]),
      );
    // Same colours in the same order as the globe's ramp (the swatch paints in its
    // currentColor), keyed by the globe's shape: quake rings, fire cells, cyclone spirals.
    expect(swatches.slice(0, 5).map(([step, colour]) => [step, colour])).toEqual(
      HAZARD_RAMP.map((hex, step) => [String(step), rgb(hex)]),
    );
    expect(swatches.map(([, , shape]) => shape)).toEqual(swatches.map(() => legend.shape));
    await list.screenshot({ path: evidencePath(testInfo, `${tag}-legend-${legend.hazard}.png`) });
  }
}

test.describe('hazard symbols over both base imageries', () => {
  // A high-density display, as analysts use: crisp close-ups of the symbol edges.
  test.use({ deviceScaleFactor: 2 });

  for (const imagery of IMAGERY) {
    test(`hazard symbols and legends read over ${imagery.id}`, async ({
      page,
      baseURL,
    }, testInfo) => {
      test.setTimeout(LIVE_IMAGERY ? 300_000 : 150_000);
      const problems = await watchPage(page, { liveImagery: LIVE_IMAGERY });
      const tiles = recordGibsTiles(page);
      const tag = imagery.id.replace('gibs-', '');
      const measured: Record<string, Partial<Record<Hazard, HazardGain>>> = {};

      // 1. Switch the base imagery through the map controls; each hazard, then all of them.
      await open(page, baseURL, document(CARIBBEAN, 'natural-earth-ii'));
      await page.getByRole('button', { name: 'Imagery', exact: true }).click();
      const choice = page.getByRole('menuitemradio', {
        name: new RegExp(`^${escaped(imagery.name)}`),
      });
      await choice.click();
      await expect(choice).toHaveAttribute('aria-checked', 'true');
      await page.keyboard.press('Escape');
      await expect(page.getByRole('menuitemradio')).toHaveCount(0);
      await imageryReady(page, tiles, imagery.layer);
      measured.caribbean = await checkView(page, CARIBBEAN_STEPS, () =>
        screenshot(page, testInfo, `${tag}-caribbean`),
      );
      await expectLegendsMatchRamp(page, testInfo, tag);
      await page.screenshot({ path: evidencePath(testInfo, `${tag}-legends.png`) });

      // 2. The fires: one detection in every FRP bin.
      await open(page, baseURL, document(HELSINKI, imagery.id));
      await imageryReady(page, tiles, imagery.layer);
      measured.helsinki = await checkView(page, HELSINKI_STEPS, () =>
        screenshot(page, testInfo, `${tag}-helsinki-fires`),
      );
      // Square cells pick like any symbol: the 80 MW detection over Helsinki (ramp step 2).
      await expect.poll(async () => (await scanRamp(page)).nearest[2]).not.toBeNull();
      const target = (await scanRamp(page)).nearest[2];
      if (!target) throw new Error('No step-2 fire cell to pick.');
      await page.locator(GLOBE_CANVAS).click({ position: target });
      const inspector = page.getByRole('region', { name: 'Active fire detection', exact: true });
      await expect(inspector).toBeVisible();
      await expect(inspector.getByText('80.0 MW', { exact: true })).toBeVisible();
      await page.waitForTimeout(300); // the halo's frame
      await screenshot(page, testInfo, `${tag}-helsinki-fire-selected`);
      await page.keyboard.press('Escape');
      await expect(inspector).toBeHidden();

      // 3. Everything from far away: each hazard, then all at once.
      await open(page, baseURL, document(OVERVIEW, imagery.id));
      await imageryReady(page, tiles, imagery.layer);
      measured.overview = await checkView(page, OVERVIEW_STEPS, () =>
        screenshot(page, testInfo, `${tag}-overview`),
      );

      const pixels = JSON.stringify({ imagery: imagery.id, liveImagery: LIVE_IMAGERY, measured });
      writeFileSync(evidencePath(testInfo, `${tag}-ramp-pixels.json`), pixels);
      await testInfo.attach('ramp-pixels.json', {
        body: JSON.stringify({ imagery: imagery.id, liveImagery: LIVE_IMAGERY, measured, tiles }),
        contentType: 'application/json',
      });
      if (LIVE_IMAGERY) {
        const own = tiles.filter((t) => t.url.includes(imagery.layer));
        expect(own.length).toBeGreaterThan(0);
        expect(own.every((t) => t.status === 200 && t.contentType.startsWith('image/'))).toBe(true);
      }
      expect(problems.errors).toEqual([]);
      expect(await problems.cspViolations()).toEqual([]);
    });
  }
});

/**
 * Rings and spirals stay selectable: an earthquake ring picks through its open centre (its
 * near-invisible wash in the layer's pick assists answers when nothing drawn is under the
 * cursor), and the cyclone spiral's eye on the storm position still selects the storm. Each
 * view centres one isolated symbol under the camera.
 */
test('quake rings select through their open centre; the cyclone spiral stays selectable', async ({
  page,
  baseURL,
}, testInfo) => {
  test.setTimeout(90_000);
  const problems = await watchPage(page);
  const canvas = page.locator(GLOBE_CANVAS);
  const centreOf = async () => {
    const box = await canvas.boundingBox();
    if (!box) throw new Error('No globe canvas.');
    return { x: box.width / 2, y: box.height / 2 };
  };

  // M3.72, 164 km NE of Cruz Bay (ramp step 1): its nearest neighbour is ~90 km away.
  await open(page, baseURL, document(pose(-63.899, 19.546, 1_500_000), 'natural-earth-ii'));
  await setHazards(page, ['earthquakes'], true);
  await expect.poll(async () => (await scanRamp(page)).nearest[1]).not.toBeNull();
  const centre = await centreOf();
  const ring = (await scanRamp(page)).nearest[1];
  // The ring's nearest stroke pixel lies off-centre: the middle is open, not a filled disc.
  const fromCentre = ring ? Math.hypot(ring.x - centre.x, ring.y - centre.y) : 0;
  expect(fromCentre).toBeGreaterThanOrEqual(3);
  expect(fromCentre).toBeLessThanOrEqual(8);
  await canvas.click({ position: centre });
  const quake = page.getByRole('region', { name: 'M 3.7 earthquake', exact: true });
  await expect(quake).toBeVisible();
  await page.waitForTimeout(300); // the halo's frame
  await screenshot(page, testInfo, 'quake-ring-selected');
  await page.keyboard.press('Escape');
  await expect(quake).toBeHidden();

  // TEST ALPHA (20° N 60° W): the spiral's eye sits on the storm position.
  await open(page, baseURL, document(pose(-60, 20, 1_500_000), 'natural-earth-ii'));
  await setHazards(page, ['cyclones'], true);
  await expect.poll(async () => (await rampPixels(page))[2] ?? 0).toBeGreaterThan(0);
  await canvas.click({ position: await centreOf() });
  await expect(page.getByRole('region', { name: 'TEST ALPHA', exact: true })).toBeVisible();
  await page.waitForTimeout(300);
  await screenshot(page, testInfo, 'cyclone-spiral-selected');
  expect(problems.errors).toEqual([]);
});

/**
 * Overlapping rings pick what is drawn: the Whites City, NM aftershocks (M2.6 and M2.9, ramp
 * step 0) lie about 4 km from an M4.0 (step 2), so at 700 km the small rings show through the
 * big ring's open centre. A click on a small ring's stroke there selects that small quake, not
 * the big ring whose centre it sits in; a click on the big ring's own stroke selects it.
 */
test('overlapping quake rings select the ring drawn under the cursor', async ({
  page,
  baseURL,
}, testInfo) => {
  test.setTimeout(90_000);
  const problems = await watchPage(page);
  const canvas = page.locator(GLOBE_CANVAS);
  // The M4.0, 56 km S of Whites City, under the camera.
  await open(page, baseURL, document(pose(-104.427, 31.666, 700_000), 'natural-earth-ii'));
  await setHazards(page, ['earthquakes'], true);
  await expect
    .poll(async () => {
      const { nearest } = await scanRamp(page);
      return nearest[0] !== null && nearest[2] !== null;
    })
    .toBe(true);
  const box = await canvas.boundingBox();
  if (!box) throw new Error('No globe canvas.');
  const centre = { x: box.width / 2, y: box.height / 2 };
  const { nearest } = await scanRamp(page);
  const small = nearest[0];
  const big = nearest[2];
  if (!small || !big) throw new Error('The Whites City rings are not drawn.');
  // The small ring's stroke lies inside the M4.0 ring's open centre (14 px ring, 10 px hole).
  expect(Math.hypot(small.x - centre.x, small.y - centre.y)).toBeLessThan(5);

  await canvas.click({ position: small });
  const smallQuake = page.getByRole('region', { name: /^M 2\.[69] earthquake$/ });
  await expect(smallQuake).toBeVisible();
  await page.waitForTimeout(300); // the halo's frame
  await screenshot(page, testInfo, 'overlapping-small-ring-selected');
  await page.keyboard.press('Escape');
  await expect(smallQuake).toBeHidden();

  await canvas.click({ position: big });
  const bigQuake = page.getByRole('region', { name: 'M 4.0 earthquake', exact: true });
  await expect(bigQuake).toBeVisible();
  await page.waitForTimeout(300);
  await screenshot(page, testInfo, 'overlapping-big-ring-selected');
  expect(problems.errors).toEqual([]);
});

interface FrameEvent {
  readonly at: number;
  readonly direction: 'sent' | 'received';
  readonly kind: string;
}

/**
 * ENG-950 evidence (not the ENG-1000 performance harness): from clicking the fire layer on to
 * the fires snapshot arriving on the stream and to the first fire pixels on the canvas, in
 * fresh page loads against the fixture server. Canvas sampling is a screenshot per check, so
 * the first-pixels time is bracketed by the last empty sample and the first painted one.
 */
test('fire layer toggle-to-drawn timing over five fresh loads', async ({
  page,
  baseURL,
}, testInfo) => {
  test.setTimeout(120_000);
  const problems = await watchPage(page);
  const frames: FrameEvent[] = [];
  page.on('websocket', (socket) => {
    const record = (direction: FrameEvent['direction']) => (data: { payload: string | Buffer }) => {
      const at = performance.now();
      try {
        const frame = JSON.parse(String(data.payload));
        const feed = frame.query?.feed ?? frame.snapshot?.feed;
        if (feed === 'fires') frames.push({ at, direction, kind: frame.op ?? frame.type });
      } catch {
        // Not JSON: not a feed frame.
      }
    };
    socket.on('framesent', record('sent'));
    socket.on('framereceived', record('received'));
  });

  const runs = [];
  for (let run = 1; run <= 5; run += 1) {
    await open(page, baseURL, document(HELSINKI, 'gibs-black-marble'));
    const fireCount = async () => (await rampPixels(page)).reduce((sum, n) => sum + n, 0);
    const baseline = await fireCount();
    const fires = page.getByRole('switch', { name: HAZARDS.fires, exact: true });
    const clicked = performance.now();
    await fires.click();
    let lastEmptyStart = clicked;
    let painted: { start: number; end: number } | null = null;
    const samples: number[] = [];
    while (!painted && performance.now() - clicked < 10_000) {
      const start = performance.now();
      const count = await fireCount();
      const end = performance.now();
      samples.push(end - start);
      if (count > baseline + 50) painted = { start, end };
      else lastEmptyStart = start;
    }
    expect(painted, `run ${run} drew fires`).not.toBeNull();
    const ms = (t: number) => Math.round(t - clicked);
    const subscribe = frames.find((f) => f.at >= clicked && f.direction === 'sent');
    const snapshot = frames.find((f) => f.at >= clicked && f.kind === 'snapshot');
    runs.push({
      run,
      subscribeSentMs: subscribe ? ms(subscribe.at) : null,
      snapshotReceivedMs: snapshot ? ms(snapshot.at) : null,
      // Nothing can be drawn before the snapshot arrives, nor before the last empty sample.
      firstPixelsAfterMs: ms(Math.max(lastEmptyStart, snapshot?.at ?? clicked)),
      firstPixelsByMs: painted ? ms(painted.end) : null,
      sampleMs: Math.round(samples.reduce((a, b) => a + b, 0) / samples.length),
    });
    await fires.click();
  }

  const evidence = {
    measured: 'click on the fire layer switch → fires snapshot frame received → first fire pixels',
    workload: 'eight synthetic FIRMS detections, Helsinki at 400 km, blank (stubbed) GIBS tiles',
    scope: 'local fixture-server acceptance evidence for ENG-950; not ENG-1000, not live data',
    runs,
  };
  writeFileSync(
    evidencePath(testInfo, 'fire-toggle-timing.json'),
    JSON.stringify(evidence, null, 2),
  );
  await testInfo.attach('fire-toggle-timing.json', {
    body: JSON.stringify(evidence),
    contentType: 'application/json',
  });
  // Every run must draw; the times are recorded, not asserted. Screenshot sampling under a
  // parallel suite on a busy machine adds seconds, so a wall-clock bound here only measures
  // the machine. The ≤ 3 s activation target is held by the ENG-1000 performance harness.
  for (const run of runs) {
    expect(run.snapshotReceivedMs, `run ${run.run} snapshot`).not.toBeNull();
    expect(run.firstPixelsByMs, `run ${run.run} drew fire pixels`).not.toBeNull();
  }
  expect(problems.errors).toEqual([]);
});
