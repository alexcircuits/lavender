/**
 * ENG-980 vessel journey against the real fixture server: the synthetic AIS replay
 * (`e2e/fixtures/aisstream.jsonl`, invented ships) through the Rust store, hub, HTTP API and
 * stream into the vessel renderer. Checks that hull chevrons and at-rest dots are actually
 * drawn in the maritime colour, that a ship can be picked and inspected with its synthetic
 * provenance and recent track (drawn as the selection trail), and that MMSI search selects it.
 *
 * Imagery tiles are stubbed by default, so the pixel checks run on a dark globe. For a review
 * over real NASA GIBS Blue Marble and Black Marble tiles (public domain, contacted only for
 * imagery; vessels still replay locally):
 *
 *   LAVENDER_E2E_LIVE_IMAGERY=1 LAVENDER_E2E_EVIDENCE_DIR=<dir outside the repo> \
 *     PLAYWRIGHT_CHANNEL=chrome pnpm run e2e vessels.spec.ts
 *
 * Screenshots go to LAVENDER_E2E_EVIDENCE_DIR when set, else to the test's output directory.
 */
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { expect, type Page, type TestInfo, test } from '@playwright/test';
import type { BBox, Catalog, ClientFrame, FeedSnapshot, Vessel } from '../src/contracts';
import { type WorkspaceDocument, workspaceShareUrl } from '../src/state/workspaceDocument';
import {
  availableLayers,
  GLOBE_CANVAS,
  gibsTilesSettled,
  recordGibsTiles,
  sourceName,
  watchPage,
} from './support';

const LIVE_IMAGERY = process.env.LAVENDER_E2E_LIVE_IMAGERY === '1';
const EVIDENCE_DIR = process.env.LAVENDER_E2E_EVIDENCE_DIR;

/** Design-system §4 maritime colour and the selection accent, as RGB. */
const MARITIME = [0x19, 0x9e, 0x70] as const;
const SELECTION = [0xb9, 0xa6, 0xff] as const;

/**
 * The Gulf of Finland ships of the replay (three under way, two at rest or with unknown
 * speed), framed clear of the search field, panels and status bar at 1280×720.
 */
const GULF = { lon: 24.68, lat: 59.83, heightM: 160_000 };
const TRADER = { mmsi: '230990001', name: 'TEST TRADER', lon: 24.96, lat: 60.1 };
const SYNTHETIC_NOTICE =
  'Synthetic AIS replay: invented test ships at their recorded report times.';

function camera(at: { lon: number; lat: number; heightM: number }) {
  return { ...at, headingRad: 0, pitchRad: -Math.PI / 2, rollRad: 0 };
}

function view(
  catalog: Catalog,
  options: {
    readonly at: { lon: number; lat: number; heightM: number };
    readonly imageryId: string;
    readonly vessels: boolean;
    readonly selection?: { feed: string; id: string };
  },
): WorkspaceDocument {
  return {
    schemaVersion: 2,
    camera: camera(options.at),
    layers: catalog.layers.map(({ feed }) => ({
      id: feed,
      enabled: feed === 'vessels' && options.vessels,
      options: {},
    })),
    imageryId: options.imageryId,
    selection: options.selection ?? null,
    annotations: [],
  };
}

async function open(page: Page, baseURL: string | undefined, doc: WorkspaceDocument) {
  await page.goto('about:blank');
  await page.goto(workspaceShareUrl(doc, baseURL ?? 'http://127.0.0.1:7412/'));
  await expect(page.getByText('Loading globe…')).toBeHidden({ timeout: 30_000 });
}

function evidencePath(testInfo: TestInfo, name: string): string {
  if (!EVIDENCE_DIR) return testInfo.outputPath(name);
  mkdirSync(EVIDENCE_DIR, { recursive: true });
  return join(EVIDENCE_DIR, name);
}

interface Blob {
  readonly area: number;
  /** Centroid in CSS pixels relative to the canvas. */
  readonly x: number;
  readonly y: number;
}

interface PixelScan {
  /** Connected regions of maritime-coloured pixels (symbol interiors), largest first. */
  readonly maritime: Blob[];
  /**
   * Selection-coloured pixels in a band 40–220 px west of the canvas centre: where the trail
   * of a ship at the centre heading east lies, clear of its halo, labels and focus rings.
   */
  readonly selectionWest: number;
}

/**
 * Reads the globe canvas pixels with the floating panels painted black. Symbol interiors are
 * the white glyph tinted to exactly the maritime colour; anti-aliased rims and imagery are
 * excluded by the tolerance, which is why counts are compared against the same view with the
 * layer off when real imagery is shown.
 */
async function scan(page: Page, tolerance = 24): Promise<PixelScan> {
  const canvas = page.locator(GLOBE_CANVAS);
  const png = await canvas.screenshot({
    mask: [
      page.getByRole('region', { name: 'Layers', exact: true }),
      page.locator('[data-inspector]'),
    ],
    maskColor: '#000000',
  });
  const box = await canvas.boundingBox();
  return page.evaluate(
    async ({ b64, maritime, selection, tolerance, cssWidth }) => {
      const image = new Image();
      image.src = `data:image/png;base64,${b64}`;
      await image.decode();
      const c = document.createElement('canvas');
      c.width = image.naturalWidth;
      c.height = image.naturalHeight;
      const ctx = c.getContext('2d');
      if (!ctx) throw new Error('Canvas pixel inspection unavailable.');
      ctx.drawImage(image, 0, 0);
      const { data } = ctx.getImageData(0, 0, c.width, c.height);
      const near = (i: number, rgb: readonly number[]) =>
        Math.abs((data[i] ?? 0) - (rgb[0] ?? 0)) <= tolerance &&
        Math.abs((data[i + 1] ?? 0) - (rgb[1] ?? 0)) <= tolerance &&
        Math.abs((data[i + 2] ?? 0) - (rgb[2] ?? 0)) <= tolerance;
      const scale = cssWidth > 0 ? c.width / cssWidth : 1;
      const mask = new Uint8Array(c.width * c.height);
      let selectionWest = 0;
      for (let p = 0; p < mask.length; p += 1) {
        const i = p * 4;
        if (near(i, maritime)) mask[p] = 1;
        else if (near(i, selection)) {
          const west = (c.width / 2 - (p % c.width)) / scale;
          const off = Math.abs(Math.floor(p / c.width) - c.height / 2) / scale;
          if (west >= 40 && west <= 220 && off <= 15) selectionWest += 1;
        }
      }
      const blobs: { area: number; x: number; y: number }[] = [];
      const stack: number[] = [];
      for (let start = 0; start < mask.length; start += 1) {
        if (mask[start] !== 1) continue;
        mask[start] = 2;
        stack.push(start);
        let area = 0;
        let sx = 0;
        let sy = 0;
        while (stack.length > 0) {
          const p = stack.pop() ?? 0;
          const x = p % c.width;
          const y = Math.floor(p / c.width);
          area += 1;
          sx += x;
          sy += y;
          for (const [dx, dy] of [
            [1, 0],
            [-1, 0],
            [0, 1],
            [0, -1],
          ] as const) {
            const nx = x + dx;
            const ny = y + dy;
            if (nx < 0 || ny < 0 || nx >= c.width || ny >= c.height) continue;
            const q = ny * c.width + nx;
            if (mask[q] !== 1) continue;
            mask[q] = 2;
            stack.push(q);
          }
        }
        blobs.push({ area: area / scale ** 2, x: sx / area / scale, y: sy / area / scale });
      }
      blobs.sort((a, b) => b.area - a.area);
      return { maritime: blobs, selectionWest };
    },
    {
      b64: png.toString('base64'),
      maritime: [...MARITIME],
      selection: [...SELECTION],
      tolerance,
      cssWidth: box?.width ?? 0,
    },
  );
}

test.afterEach(async ({ page }, testInfo) => {
  if (testInfo.status !== testInfo.expectedStatus) {
    await page.screenshot({ path: testInfo.outputPath('failure.png') }).catch(() => undefined);
  }
});

/** Chevron interiors are ~60 px², dots ~30 px² (vesselGlyphs.ts); specks are ignored. */
const CHEVRON_MIN_AREA = 40;
const DOT_MIN_AREA = 12;
const chevrons = (s: PixelScan) => s.maritime.filter((b) => b.area >= CHEVRON_MIN_AREA);
const symbols = (s: PixelScan) => s.maritime.filter((b) => b.area >= DOT_MIN_AREA);
const dots = (s: PixelScan) =>
  s.maritime.filter((b) => b.area >= DOT_MIN_AREA && b.area < CHEVRON_MIN_AREA);

test('AIS fixture ships draw as chevrons and dots, inspect with their track, and select by MMSI', async ({
  page,
  request,
  baseURL,
}, testInfo) => {
  test.setTimeout(120_000);
  const { catalog, layers } = await availableLayers(request, ['vessels']);
  const vesselLayer = layers.vessels.name;
  const problems = await watchPage(page);

  // The replay through the real server: five ships in the gulf, synthetic provenance.
  const response = await request.get('/api/v1/feeds/vessels?bbox=22,59,26,61');
  expect(response.ok()).toBe(true);
  const snapshot = (await response.json()) as FeedSnapshot;
  expect(snapshot.notices).toContain('Recorded fixture data, not live.');
  expect(snapshot.notices).toContain(SYNTHETIC_NOTICE);
  const ships = (snapshot.items.items as Vessel[]).map((v) => v.mmsi).sort();
  expect(ships).toEqual(['230990001', '230990002', '230990004', '230990005', '276990003']);

  await open(
    page,
    baseURL,
    view(catalog, { at: GULF, imageryId: 'gibs-black-marble', vessels: false }),
  );
  await expect(page.getByText('Camera at 160 km', { exact: true })).toBeVisible();
  const off = await scan(page);
  expect(off.maritime.filter((b) => b.area >= DOT_MIN_AREA)).toEqual([]);

  const toggle = page.getByRole('switch', { name: vesselLayer, exact: true });
  await toggle.click();
  await expect
    .poll(async () => symbols(await scan(page)).length, { timeout: 15_000 })
    .toBeGreaterThanOrEqual(5);
  const on = await scan(page);
  await testInfo.attach('vessel-pixels.json', {
    body: JSON.stringify({ off: off.maritime.slice(0, 10), on: on.maritime.slice(0, 10) }),
    contentType: 'application/json',
  });
  // TRADER, FERRY and SAILOR make way; TANKER is at anchor and SENTINEL reports no speed.
  expect(chevrons(on)).toHaveLength(3);
  expect(dots(on)).toHaveLength(2);
  const row = page.locator('[data-feed="vessels"]');
  await expect(row).toContainText('5');
  await page.getByRole('button', { name: `${vesselLayer} options`, exact: true }).click();
  const legend = page.getByRole('list', { name: 'Vessel symbol legend' });
  await expect(legend).toContainText('Under way, turned to its course (or heading)');
  await expect(legend).toContainText('At rest (under 0.5 kn) or speed not reported');
  await expect(page.getByText('5 in view · 7 known to the server', { exact: true })).toBeVisible();
  await page.screenshot({ path: evidencePath(testInfo, 'vessels-gulf-legend.png') });

  // Pick a chevron on the canvas: the inspector opens on that ship.
  const target = chevrons(on)[0];
  if (!target) throw new Error('no chevron to pick');
  await page.locator(GLOBE_CANVAS).click({ position: { x: target.x, y: target.y } });
  const picked = page.locator('[data-inspector]');
  await expect(picked).toBeVisible();
  await expect(picked).toContainText(/MMSI 2309900\d\d/);

  // Search by MMSI selects the ferry (local and server hits) and flies to it.
  const search = page.getByRole('combobox', { name: 'Search places, coordinates and callsigns' });
  await search.click();
  await search.fill('230990002');
  await expect(page.getByRole('option', { name: /TEST FERRY/ }).first()).toBeVisible();
  await search.press('Enter');
  const ferry = page.getByRole('region', { name: 'TEST FERRY', exact: true });
  await expect(ferry).toBeVisible();
  await expect(ferry.getByText('Passenger', { exact: true })).toBeVisible();
  await expect(ferry.getByText('4 positions over 6 min · AISStream')).toBeVisible();
  expect(problems.errors).toEqual([]);
  expect(await problems.cspViolations()).toEqual([]);
});

test('the selected ship shows its facts, synthetic provenance and recent track as a trail', async ({
  page,
  request,
  baseURL,
}, testInfo) => {
  test.setTimeout(120_000);
  const { catalog } = await availableLayers(request, ['vessels']);
  const problems = await watchPage(page);
  // Close in over the trader, selected through a shared view: its 3.3 km track runs west.
  await open(
    page,
    baseURL,
    view(catalog, {
      at: { lon: TRADER.lon, lat: TRADER.lat, heightM: 15_000 },
      imageryId: 'gibs-black-marble',
      vessels: true,
      selection: { feed: 'vessels', id: TRADER.mmsi },
    }),
  );
  const inspector = page.getByRole('region', { name: TRADER.name, exact: true });
  await expect(inspector).toBeVisible({ timeout: 15_000 });
  for (const [label, value] of [
    ['MMSI', TRADER.mmsi],
    ['IMO', '9999981'],
    ['Call sign', 'XTEST'],
    ['Vessel type', 'Cargo'],
    ['Navigational status', 'Under way using engine'],
    ['Destination', 'FIHEL'],
    ['ETA', '15 Oct 18:00 UTC'],
    ['Draught', '6.2 m'],
    ['Length × beam', '70 × 12 m'],
    ['Speed over ground', '11.5 kn'],
    ['Course over ground', '88°'],
    ['Heading', '89°'],
    ['Position time', '2026-10-15 12:00:00 UTC'],
    ['Source', sourceName(catalog, 'aisstream')],
  ] as const) {
    const term = inspector.locator('dt', { hasText: new RegExp(`^${label}$`) }).first();
    await expect(term.locator('xpath=following-sibling::dd[1]')).toHaveText(value);
  }
  await expect(inspector.locator('dd', { hasText: SYNTHETIC_NOTICE })).toBeVisible();
  await expect(inspector.getByText('7 positions over 10 min · AISStream')).toBeVisible();
  const track = inspector.getByRole('table', { name: 'Vessel track' });
  await expect(track.getByRole('row')).toHaveCount(8);
  await expect(track.getByRole('row').nth(1)).toContainText('12:00:00 UTC');

  // The trail: selection-coloured pixels west of the ship (at the canvas centre), clear of
  // its halo. Gone when the inspector closes.
  await expect
    .poll(async () => (await scan(page)).selectionWest, { timeout: 15_000 })
    .toBeGreaterThan(60);
  const selected = await scan(page);
  await page.screenshot({ path: evidencePath(testInfo, 'vessel-trader-track.png') });

  await inspector.getByRole('button', { name: 'Close inspector' }).click();
  await expect(inspector).toBeHidden();
  await expect.poll(async () => (await scan(page)).selectionWest).toBe(0);
  await testInfo.attach('vessel-trail-pixels.json', {
    body: JSON.stringify({ selectedWest: selected.selectionWest, trader: TRADER }),
    contentType: 'application/json',
  });
  expect(problems.errors).toEqual([]);
  expect(await problems.cspViolations()).toEqual([]);
});

test('a continental view over the Baltic still queries and counts the gulf ships', async ({
  page,
  request,
  baseURL,
}, testInfo) => {
  test.setTimeout(90_000);
  const { catalog, layers } = await availableLayers(request, ['vessels']);
  const problems = await watchPage(page);
  // The viewport the page asks the server for (subscribe and update frames).
  const queried: (BBox | null)[] = [];
  page.on('websocket', (ws) =>
    ws.on('framesent', ({ payload }) => {
      const frame = JSON.parse(String(payload)) as ClientFrame;
      if (frame.op !== 'unsubscribe' && frame.query.feed === 'vessels') {
        queried.push(frame.query.bbox ?? null);
      }
    }),
  );
  // 5 000 km over the gulf: Cesium's corner-only view rectangle used to end near 51° N here,
  // so the query left out the very ships at the centre of the view (ENG-980 review).
  await open(
    page,
    baseURL,
    view(catalog, {
      at: { lon: GULF.lon, lat: GULF.lat, heightM: 5_000_000 },
      imageryId: 'gibs-black-marble',
      vessels: true,
    }),
  );
  await expect(page.getByText(/^Camera at 5\s000 km$/)).toBeVisible();
  const row = page.locator('[data-feed="vessels"]');
  await expect(row).toContainText(/(^|\D)5\s*vessels/, { timeout: 15_000 });
  const last = queried.at(-1);
  expect(last === null || (last !== undefined && last.south < 59.5 && last.north > 60.15)).toBe(
    true,
  );
  await page.getByRole('button', { name: `${layers.vessels.name} options`, exact: true }).click();
  await expect(page.getByText(/This view is wide: crowded spots show one ship each/)).toBeVisible();
  await page.screenshot({ path: evidencePath(testInfo, 'vessels-continental.png') });
  expect(problems.errors).toEqual([]);
});

for (const [imageryId, gibsLayer] of [
  ['gibs-blue-marble', 'BlueMarble_ShadedRelief_Bathymetry'],
  ['gibs-black-marble', 'VIIRS_Black_Marble'],
] as const) {
  test(`vessels read over ${imageryId}`, async ({ page, request, baseURL }, testInfo) => {
    test.setTimeout(LIVE_IMAGERY ? 180_000 : 90_000);
    const { catalog, layers } = await availableLayers(request, ['vessels']);
    const problems = await watchPage(page, { liveImagery: LIVE_IMAGERY });
    const tiles = recordGibsTiles(page);
    const tag = imageryId.replace('gibs-', '');
    await open(page, baseURL, view(catalog, { at: GULF, imageryId, vessels: false }));
    if (LIVE_IMAGERY) await gibsTilesSettled(page, tiles, gibsLayer);
    const before = await scan(page);
    await page.getByRole('switch', { name: layers.vessels.name, exact: true }).click();
    await expect
      .poll(async () => chevrons(await scan(page)).length, { timeout: 15_000 })
      .toBeGreaterThanOrEqual(3);
    const after = await scan(page);
    // Real imagery may carry maritime-like pixels of its own: count the gain over "off".
    expect(chevrons(after).length - chevrons(before).length).toBeGreaterThanOrEqual(3);
    expect(dots(after).length - dots(before).length).toBeGreaterThanOrEqual(2);
    await page.screenshot({ path: evidencePath(testInfo, `vessels-${tag}.png`) });

    await page.locator(GLOBE_CANVAS).click({
      position: { x: chevrons(after)[0]?.x ?? 0, y: chevrons(after)[0]?.y ?? 0 },
    });
    await expect(page.locator('[data-inspector]')).toBeVisible();
    await expect(page.locator('[data-inspector]')).toContainText(
      /\d+ positions? · AISStream|kept no positions/,
    );
    await page.screenshot({ path: evidencePath(testInfo, `vessels-${tag}-selected.png`) });
    await testInfo.attach(`vessels-${tag}.json`, {
      body: JSON.stringify({
        imageryId,
        liveImagery: LIVE_IMAGERY,
        before: before.maritime.slice(0, 12),
        after: after.maritime.slice(0, 12),
        gibsTiles: tiles.filter((t) => t.url.includes(gibsLayer)).length,
      }),
      contentType: 'application/json',
    });
    expect(problems.errors).toEqual([]);
    expect(await problems.cspViolations()).toEqual([]);
  });
}
