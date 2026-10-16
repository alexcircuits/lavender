/**
 * ENG-990 launches slice: launch-pad markers, the launch list and the launch/pad inspector
 * against the real lavender-server replaying `e2e/fixtures/launches.json` (three synthetic
 * Launch Library 2 launches: two from Space Launch Complex 40, one from 4E).
 *
 * The page clock is fixed at 2026-06-02 12:00 UTC so the list's upcoming/recent split and its
 * relative times are deterministic: Sirius SXM-11 (1 June) is recent, Starlink Group 17-40
 * (2 June 18:00) and Cygnus NG-21 (4 August) are upcoming. Fixture mode matches no satellites
 * (the fixture launches carry no launch designator): the journey checks the honest "none
 * matched" sheet, and a separate test gives one launch matches on the WebSocket on its way to
 * the page (`page.routeWebSocket`) to follow a matched-satellite link to the satellite.
 *
 * Screenshots go to LAVENDER_E2E_EVIDENCE_DIR when set (else the test's output directory).
 * NASA GIBS tiles are stubbed unless LAVENDER_E2E_LIVE_IMAGERY=1 (the visual review run).
 */
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { expect, type Page, type TestInfo, test } from '@playwright/test';
import type { FeedSnapshot, Launch, ServerFrame } from '../src/contracts';
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
const FIXTURE_CLOCK = new Date('2026-06-02T12:00:00Z');
const NOTICE = 'Recorded fixture data, not live.';
/** `--map-space` and `--map-selection` (docs/design/design-system.md §4). */
const SPACE = [0xd6, 0xd3, 0xe6] as const;
const SELECTION = [0xb9, 0xa6, 0xff] as const;

const IMAGERY = [
  { id: 'gibs-blue-marble', layer: 'BlueMarble_ShadedRelief_Bathymetry' },
  { id: 'gibs-black-marble', layer: 'VIIRS_Black_Marble' },
] as const;

/**
 * North America with both fixture pads (Florida, California) in view, the Cape east of centre
 * but clear of the inspector that opens on the right.
 */
function viewOf(imageryId: string, lon = -92): WorkspaceDocument {
  return {
    schemaVersion: 2,
    camera: {
      lon,
      lat: 31,
      heightM: 7_000_000,
      headingRad: 0,
      pitchRad: -Math.PI / 2,
      rollRad: 0,
    },
    layers: [
      { id: 'launches', enabled: false, options: {} },
      { id: 'satellites', enabled: false, options: {} },
    ],
    imageryId,
    selection: null,
    annotations: [],
  };
}

function evidencePath(testInfo: TestInfo, name: string): string {
  if (!EVIDENCE_DIR) return testInfo.outputPath(name);
  mkdirSync(EVIDENCE_DIR, { recursive: true });
  return join(EVIDENCE_DIR, name);
}

interface ColourScan {
  readonly count: number;
  /** Centres of the matching pixel clusters, west to east, in CSS pixels of the canvas. */
  readonly clusters: readonly { readonly x: number; readonly y: number }[];
}

/**
 * Pixels of the globe canvas within `tolerance` of `rgb` on every channel, grouped into
 * clusters, with the chrome that floats over the globe painted black: the panels (their text
 * and icons are light too), the credits line (it names the layer's source) and the feed chip.
 */
async function scanColour(
  page: Page,
  rgb: readonly [number, number, number],
  tolerance = 12,
): Promise<ColourScan> {
  const png = await page.locator(GLOBE_CANVAS).screenshot({
    mask: [
      page.locator('section[aria-labelledby]'),
      page.locator('[class*="creditsSlot"]'),
      page.locator('[class*="chipSlot"]'),
    ],
    maskColor: '#000000',
  });
  const box = await page.locator(GLOBE_CANVAS).boundingBox();
  return page.evaluate(
    async ({ b64, rgb, tolerance, cssWidth }) => {
      const image = new Image();
      image.src = `data:image/png;base64,${b64}`;
      await image.decode();
      const canvas = document.createElement('canvas');
      canvas.width = image.naturalWidth;
      canvas.height = image.naturalHeight;
      const ctx = canvas.getContext('2d');
      if (!ctx) throw new Error('Canvas pixel inspection unavailable.');
      ctx.drawImage(image, 0, 0);
      const { data } = ctx.getImageData(0, 0, canvas.width, canvas.height);
      const scale = cssWidth > 0 ? canvas.width / cssWidth : 1;
      const points: { x: number; y: number }[] = [];
      for (let i = 0; i < data.length; i += 4) {
        if (
          Math.abs((data[i] ?? 0) - rgb[0]) <= tolerance &&
          Math.abs((data[i + 1] ?? 0) - rgb[1]) <= tolerance &&
          Math.abs((data[i + 2] ?? 0) - rgb[2]) <= tolerance
        ) {
          points.push({ x: (i / 4) % canvas.width, y: Math.floor(i / 4 / canvas.width) });
        }
      }
      // Markers are far apart at this height: split clusters on 30 px gaps along x.
      const gap = 30 * scale;
      const sorted = [...points].sort((a, b) => a.x - b.x);
      const clusters: { x: number; y: number; n: number }[] = [];
      let last = Number.NEGATIVE_INFINITY;
      for (const p of sorted) {
        const current = clusters.at(-1);
        if (!current || p.x - last > gap) clusters.push({ x: p.x, y: p.y, n: 1 });
        else {
          current.x += p.x;
          current.y += p.y;
          current.n += 1;
        }
        last = p.x;
      }
      return {
        count: points.length,
        clusters: clusters
          .filter((c) => c.n >= 8)
          .map((c) => ({ x: c.x / c.n / scale, y: c.y / c.n / scale })),
      };
    },
    { b64: png.toString('base64'), rgb, tolerance, cssWidth: box?.width ?? 0 },
  );
}

async function open(page: Page, baseURL: string | undefined, doc: WorkspaceDocument) {
  // The list groups and relative times read the page clock; feeds and rendering do not.
  await page.clock.setFixedTime(FIXTURE_CLOCK);
  await page.goto(workspaceShareUrl(doc, baseURL ?? 'http://127.0.0.1:7442/'));
  await expect(page.getByText('Loading globe…')).toBeHidden({ timeout: 30_000 });
  // formatCount groups thousands with a narrow no-break space.
  await expect(page.getByText(/^Camera at 7\s000 km$/)).toBeVisible();
}

/**
 * Shows or hides the left island through the rail, so pixel scans see the whole globe. Focus
 * and the pointer leave the rail afterwards: its tooltip is light text over the globe.
 */
async function setLayersPanel(page: Page, open: boolean) {
  const panel = page.getByRole('region', { name: 'Layers', exact: true });
  if ((await panel.isVisible()) !== open) {
    await page.getByRole('button', { name: 'Layers', exact: true }).click();
  }
  await expect(panel).toBeVisible({ visible: open });
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await page.mouse.move(0, 0);
  await expect(page.getByRole('tooltip')).toHaveCount(0);
}

/** Clusters of `on` with no cluster of `off` within 20 px: symbols that appeared. */
function appeared(on: ColourScan, off: ColourScan): ColourScan['clusters'] {
  return on.clusters.filter((c) =>
    off.clusters.every((o) => Math.hypot(c.x - o.x, c.y - o.y) > 20),
  );
}

/**
 * Turns the launches layer on and waits until both pads are drawn (pixels, not state),
 * comparing the same view with the left island closed before and after. Leaves it closed and
 * returns the pad markers that appeared, west to east.
 */
async function turnOnLaunches(page: Page, layerName: string): Promise<ColourScan> {
  await setLayersPanel(page, false);
  const off = await scanColour(page, SPACE);
  await setLayersPanel(page, true);
  await page.getByRole('switch', { name: layerName, exact: true }).click();
  await setLayersPanel(page, false);
  await expect
    .poll(async () => appeared(await scanColour(page, SPACE), off).length, {
      message: 'two pad markers drawn in the Space colour',
      timeout: 15_000,
    })
    .toBeGreaterThanOrEqual(2);
  const on = await scanColour(page, SPACE);
  expect(on.count - off.count, `space pixels off ${off.count}, on ${on.count}`).toBeGreaterThan(80);
  return { count: on.count, clusters: appeared(on, off) };
}

test('launch pads, the launch list and the launch/pad inspector on the fixture clock', async ({
  page,
  request,
  baseURL,
}, testInfo) => {
  test.setTimeout(120_000);
  const { catalog, layers } = await availableLayers(request, ['launches', 'satellites']);
  const layerName = layers.launches.name;
  const source = sourceName(catalog, 'launch-library-2');
  const attribution = catalog.sources.find((s) => s.id === 'launch-library-2')?.attribution;

  // The fixture the server replays: three launches, worldwide (no viewport), honestly labelled.
  const response = await request.get('/api/v1/feeds/launches');
  expect(response.ok()).toBe(true);
  const snapshot = (await response.json()) as FeedSnapshot;
  expect(snapshot.notices).toContain(NOTICE);
  const launches = snapshot.items.items as Launch[];
  expect(launches.map((l) => l.mission)).toEqual([
    'Sirius SXM-11',
    'Starlink Group 17-40',
    'Cygnus NG-21',
  ]);
  expect(launches.every((l) => l.matchedNoradIds.length === 0)).toBe(true);

  const problems = await watchPage(page);
  await open(page, baseURL, viewOf('gibs-black-marble'));

  // 1. Markers: one per pad in the Space colour, the busy pad (Cape, east) badged.
  const drawn = await turnOnLaunches(page, layerName);
  await page.screenshot({ path: evidencePath(testInfo, 'launches-pads-overview.png') });

  // 2. The list: upcoming soonest first, recent below, with relative and UTC times.
  await setLayersPanel(page, true);
  await page.getByRole('button', { name: `${layerName} options`, exact: true }).click();
  await page.getByRole('button', { name: 'Show launch list', exact: true }).click();
  const panel = page.getByRole('region', { name: layerName, exact: true });
  await expect(panel).toBeVisible();
  await expect(panel.getByText(NOTICE)).toBeVisible();
  await expect(panel.getByText(`Source: ${source}`)).toBeVisible();
  const upcoming = panel.getByRole('listbox', { name: 'Upcoming launches' }).getByRole('option');
  const recent = panel.getByRole('listbox', { name: 'Recent launches' }).getByRole('option');
  await expect(upcoming).toHaveCount(2);
  await expect(recent).toHaveCount(1);
  await expect(upcoming.nth(0)).toContainText('Starlink Group 17-40');
  await expect(upcoming.nth(0)).toContainText('in 6 h');
  await expect(upcoming.nth(0)).toContainText('2026-06-02 18:00 UTC');
  await expect(upcoming.nth(0)).toContainText('Go');
  await expect(upcoming.nth(0)).toContainText('Space Launch Complex 4E');
  await expect(upcoming.nth(1)).toContainText('Cygnus NG-21');
  await expect(upcoming.nth(1)).toContainText('in 63 d');
  await expect(recent.nth(0)).toContainText('Sirius SXM-11');
  await expect(recent.nth(0)).toContainText('24 h ago');
  await expect(recent.nth(0)).toContainText('Success');
  await expect(recent.nth(0)).toContainText('SpaceX · Falcon 9 Block 5');
  await panel.screenshot({ path: evidencePath(testInfo, 'launches-list.png') });

  // 3. Pick the Cape pad on the globe (two launches): the pad sheet lists both.
  const cape = drawn.clusters.at(-1);
  if (!cape) throw new Error('No pad marker to pick.');
  await page.locator(GLOBE_CANVAS).click({ position: cape });
  const padSheet = page.getByRole('region', { name: 'Space Launch Complex 40', exact: true });
  await expect(padSheet).toBeVisible();
  await expect(padSheet.getByText('Launch pad · Cape Canaveral SFS, FL, USA')).toBeVisible();
  await expect(padSheet.getByText('2 · 1 upcoming, 1 recent')).toBeVisible();
  await expect(padSheet.getByText(NOTICE)).toBeVisible();
  await expect
    .poll(async () => (await scanColour(page, SELECTION, 10)).count, {
      message: 'the selection halo is drawn',
    })
    .toBeGreaterThan(20);
  await page.screenshot({ path: evidencePath(testInfo, 'launches-pad-selected.png') });

  // 4. A launch from the pad sheet: every Launch field, payloads, attribution, no matches.
  await padSheet
    .getByRole('list', { name: 'Recent launches from this pad' })
    .getByRole('button', { name: 'Sirius SXM-11' })
    .click();
  const sxm = page.getByRole('region', { name: 'Sirius SXM-11', exact: true });
  await expect(sxm).toBeVisible();
  const facts = sxm.getByLabel('Facts');
  for (const text of [
    'Success',
    '2026-06-01 12:00:00 UTC · 24 h ago',
    '2026-06-01 11:30–13:30 UTC (2 h)',
    'SpaceX',
    'Falcon 9 Block 5',
    'Falcon 9 Block 5 | Sirius SXM-11',
    'Space Launch Complex 40',
    'Cape Canaveral SFS, FL, USA',
  ]) {
    await expect(facts.getByText(text, { exact: true })).toBeVisible();
  }
  await expect(sxm.getByText('High-powered broadcasting satellite for SiriusXM.')).toBeVisible();
  await expect(sxm.getByRole('list', { name: 'Payloads' })).toHaveText('SXM-11');
  await expect(sxm.getByText(/^No tracked satellites are matched to this launch\./)).toBeVisible();
  await expect(sxm.getByRole('list', { name: 'Matched satellites' })).toHaveCount(0);
  if (attribution) await expect(sxm.getByText(attribution, { exact: true })).toBeVisible();
  await expect(sxm.getByLabel('Provenance').getByText(source, { exact: true })).toBeVisible();
  await expect(sxm.getByText(NOTICE)).toBeVisible();
  await sxm.screenshot({ path: evidencePath(testInfo, 'launches-inspector-launch.png') });

  // 5. Select from the list: the camera flies to the pad, the row is marked; keyboard too.
  await upcoming.nth(0).click();
  await expect(
    page.getByRole('region', { name: 'Starlink Group 17-40', exact: true }),
  ).toBeVisible();
  await expect(upcoming.nth(0)).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByText('Camera at 400 km', { exact: true })).toBeVisible({
    timeout: 10_000,
  });
  await expect.poll(async () => (await scanColour(page, SELECTION, 10)).count).toBeGreaterThan(20);
  await page.screenshot({ path: evidencePath(testInfo, 'launches-list-selected.png') });
  await upcoming.nth(0).focus();
  await page.keyboard.press('j');
  await expect(upcoming.nth(1)).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('region', { name: 'Cygnus NG-21', exact: true })).toBeVisible();

  expect(problems.errors).toEqual([]);
  expect(await problems.cspViolations()).toEqual([]);

  // 6. Turning the layer off clears the selection and the markers.
  await page.getByRole('button', { name: 'Show layers', exact: true }).click();
  await page.getByRole('switch', { name: layerName, exact: true }).click();
  await expect(page.getByRole('region', { name: 'Cygnus NG-21', exact: true })).toBeHidden();
  await expect.poll(async () => (await scanColour(page, SPACE)).clusters.length).toBe(0);
});

test.describe('launch pads over both base imageries', () => {
  // A high-density display, as analysts use: crisp close-ups of the glyph and badge.
  test.use({ deviceScaleFactor: 2 });

  for (const imagery of IMAGERY) {
    test(`pad markers read over ${imagery.id}`, async ({ page, request, baseURL }, testInfo) => {
      test.setTimeout(LIVE_IMAGERY ? 180_000 : 90_000);
      const { layers } = await availableLayers(request, ['launches']);
      const problems = await watchPage(page, { liveImagery: LIVE_IMAGERY });
      const tiles = recordGibsTiles(page);
      // The Cape near the centre, so its badge, halo and label are clear of the inspector.
      await open(page, baseURL, viewOf(imagery.id, -86));
      if (LIVE_IMAGERY) await gibsTilesSettled(page, tiles, imagery.layer);
      const tag = imagery.id.replace('gibs-', '');

      const drawn = await turnOnLaunches(page, layers.launches.name);
      expect(drawn.clusters.length).toBeGreaterThanOrEqual(2);
      await page.screenshot({ path: evidencePath(testInfo, `launches-${tag}.png`), scale: 'css' });

      // The busy pad, selected, close up: badge, halo and name label.
      const cape = drawn.clusters.at(-1);
      if (!cape) throw new Error('No pad marker to pick.');
      await page.locator(GLOBE_CANVAS).click({ position: cape });
      await expect(
        page.getByRole('region', { name: 'Space Launch Complex 40', exact: true }),
      ).toBeVisible();
      await page.waitForTimeout(300); // the halo's frame
      const box = await page.locator(GLOBE_CANVAS).boundingBox();
      if (box) {
        await page.screenshot({
          path: evidencePath(testInfo, `launches-${tag}-cape-closeup.png`),
          clip: { x: box.x + cape.x - 120, y: box.y + cape.y - 60, width: 240, height: 120 },
          scale: 'device',
        });
      }
      expect(problems.errors).toEqual([]);
      expect(await problems.cspViolations()).toEqual([]);
    });
  }
});

/** The fixture station's catalogue number in `celestrak-stations.json` ("TEST STATION"). */
const FIXTURE_STATION = 25544;
/** A catalogue number no fixture group carries. */
const UNTRACKED = 99999;

/**
 * Passes the real stream through, but gives one launch matched satellites on the way, as the
 * server's matcher would once LL2 gives a launch its designator. Fixture launches carry none,
 * so the server itself matches nothing in fixture mode.
 */
async function matchSatellites(page: Page, launchId: string, noradIds: readonly number[]) {
  await page.routeWebSocket(/\/api\/v1\/stream$/, (ws) => {
    const server = ws.connectToServer();
    ws.onMessage((message) => server.send(message));
    server.onMessage((message) => {
      if (typeof message !== 'string') {
        ws.send(message);
        return;
      }
      const frame = JSON.parse(message) as ServerFrame;
      if (frame.type !== 'snapshot' || frame.snapshot.items.kind !== 'launches') {
        ws.send(message);
        return;
      }
      const items = frame.snapshot.items.items.map((launch) =>
        launch.id === launchId ? { ...launch, matchedNoradIds: [...noradIds] } : launch,
      );
      const snapshot = { ...frame.snapshot, items: { kind: 'launches' as const, items } };
      ws.send(JSON.stringify({ ...frame, snapshot }));
    });
  });
}

test('a matched satellite selects the satellite once the satellites layer has it', async ({
  page,
  request,
  baseURL,
}, testInfo) => {
  test.setTimeout(90_000);
  const { layers } = await availableLayers(request, ['launches', 'satellites']);
  const satellitesName = layers.satellites.name;
  const response = await request.get('/api/v1/feeds/launches');
  const launches = ((await response.json()) as FeedSnapshot).items.items as Launch[];
  const sirius = launches.find((l) => l.mission === 'Sirius SXM-11');
  if (!sirius) throw new Error('The fixture has no Sirius SXM-11 launch.');

  await matchSatellites(page, sirius.id, [FIXTURE_STATION, UNTRACKED]);
  const problems = await watchPage(page);
  const doc = viewOf('gibs-black-marble');
  await open(page, baseURL, {
    ...doc,
    layers: [
      { id: 'launches', enabled: true, options: {} },
      { id: 'satellites', enabled: false, options: {} },
    ],
    selection: { feed: 'launches', id: sirius.id },
  });

  // Satellites off: the matches are listed by number, with the way to select them.
  const sheet = page.getByRole('region', { name: 'Sirius SXM-11', exact: true });
  await expect(sheet).toBeVisible({ timeout: 15_000 });
  const matched = sheet.getByRole('list', { name: 'Matched satellites' });
  await expect(matched.getByText(`NORAD ${FIXTURE_STATION}`, { exact: true })).toBeVisible();
  await expect(matched.getByRole('button')).toHaveCount(0);
  await expect(sheet.getByText(`Turn on ${satellitesName} to select them.`)).toBeVisible();

  // Turned on: the station the layer has is a link; the untracked number says why it is not.
  await sheet.getByRole('button', { name: `Turn on ${satellitesName}`, exact: true }).click();
  const link = matched.getByRole('button', { name: `TEST STATION · NORAD ${FIXTURE_STATION}` });
  await expect(link).toBeVisible({ timeout: 15_000 });
  await expect(
    matched.getByRole('listitem').filter({ hasText: `NORAD ${UNTRACKED}` }),
  ).toContainText('Not in the groups shown');
  await expect(sheet.getByText(/^Some are not in the .* groups shown/)).toBeVisible();
  await sheet.screenshot({ path: evidencePath(testInfo, 'launches-matched-satellites.png') });

  // The link selects the satellite and opens its sheet.
  await link.click();
  const satellite = page.getByRole('region', { name: 'TEST STATION', exact: true });
  await expect(satellite).toBeVisible();
  await expect(satellite.getByText(new RegExp(`^NORAD ${FIXTURE_STATION} · `))).toBeVisible();
  expect(problems.errors).toEqual([]);
  expect(await problems.cspViolations()).toEqual([]);
});

/** WebGL draw calls since the page loaded (counted by `countDraws`). */
const drawCount = (page: Page) =>
  page.evaluate(() => (window as Window & { __draws?: number }).__draws ?? 0);

/** Counts every WebGL draw call from the first frame on, as smoke.spec.ts does. */
async function countDraws(page: Page) {
  await page.addInitScript(() => {
    const counter = window as Window & { __draws?: number };
    counter.__draws = 0;
    for (const proto of [WebGLRenderingContext.prototype, WebGL2RenderingContext.prototype]) {
      for (const name of [
        'drawArrays',
        'drawElements',
        'drawArraysInstanced',
        'drawElementsInstanced',
      ] as const) {
        const original = (proto as unknown as Record<string, (...args: unknown[]) => unknown>)[
          name
        ];
        if (typeof original !== 'function') continue;
        (proto as unknown as Record<string, unknown>)[name] = function (
          this: unknown,
          ...args: unknown[]
        ) {
          counter.__draws = (counter.__draws ?? 0) + 1;
          return original.apply(this, args);
        };
      }
    }
  });
}

/**
 * Waits until the globe has drawn nothing for `quietMs` and returns the draw count then. A
 * globe that never stops drawing never gets there. One quiet second is not enough: after a
 * load Cesium redraws once more by itself (a sub-epsilon camera change about 1.5 s after the
 * tiles settle, then the camera's moveEnd 500 ms later), with or without launch pads.
 */
async function drawsOnceQuiet(page: Page, quietMs: number): Promise<number> {
  let last = -1;
  let changedAt = Date.now();
  await expect
    .poll(
      async () => {
        const now = await drawCount(page);
        if (now !== last) {
          last = now;
          changedAt = Date.now();
        }
        return Date.now() - changedAt;
      },
      {
        message: `the globe draws nothing for ${quietMs} ms once the pads are drawn`,
        timeout: 20_000,
        intervals: [250],
      },
    )
    .toBeGreaterThanOrEqual(quietMs);
  return last;
}

test('an idle globe with launch pads drawn renders nothing (request-render mode)', async ({
  page,
  request,
  baseURL,
}) => {
  test.setTimeout(60_000);
  const { layers } = await availableLayers(request, ['launches']);
  await countDraws(page);
  await watchPage(page);
  await open(page, baseURL, viewOf('gibs-black-marble'));
  await turnOnLaunches(page, layers.launches.name);
  expect(await drawCount(page)).toBeGreaterThan(0);
  const before = await drawsOnceQuiet(page, 3_000);
  await page.waitForTimeout(2_000);
  expect(await drawCount(page), 'draws while idle with pads drawn').toBe(before);
});
