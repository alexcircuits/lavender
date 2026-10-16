import { performance } from 'node:perf_hooks';
import { expect, type Page, test } from '@playwright/test';
import type { FireCell } from '../src/contracts';
import { type WorkspaceDocument, workspaceShareUrl } from '../src/state/workspaceDocument';
import {
  availableLayers,
  canvasScreenshot,
  GLOBE_CANVAS,
  gibsTilesSettled,
  recordGibsTiles,
  sourceName,
  watchPage,
} from './support';

const view: WorkspaceDocument = {
  schemaVersion: 2,
  camera: {
    lon: 24.94,
    lat: 60.17,
    heightM: 400_000,
    headingRad: 0,
    pitchRad: -Math.PI / 2,
    rollRad: 0,
  },
  layers: [
    { id: 'earthquakes', enabled: false, options: {} },
    { id: 'fires', enabled: false, options: {} },
  ],
  imageryId: 'gibs-black-marble',
  selection: null,
  annotations: [],
};

/**
 * The eight synthetic detections around Helsinki. A viewport of at most 5° keeps individual
 * detections; wider viewports and unbounded queries group them into grid cells by design
 * (crates/lavender-core/src/providers/firms.rs `aggregate_fires`).
 */
const HELSINKI_BBOX = '23,59.5,27,61.5';

/**
 * Read actual rendered canvas pixels, independently of feed counts or renderer state. The
 * floating Layers panel is painted out: its hazard icons and legend swatches are orange too.
 */
async function orangePixels(page: Page) {
  const png = await canvasScreenshot(page);
  return page.evaluate(async (base64) => {
    const image = new Image();
    image.src = `data:image/png;base64,${base64}`;
    await image.decode();
    const canvas = document.createElement('canvas');
    canvas.width = image.naturalWidth;
    canvas.height = image.naturalHeight;
    const ctx = canvas.getContext('2d');
    if (!ctx) throw new Error('Canvas pixel inspection unavailable.');
    ctx.drawImage(image, 0, 0);
    const { data } = ctx.getImageData(0, 0, canvas.width, canvas.height);
    let count = 0;
    let nearest = { x: 0, y: 0, distance: Number.POSITIVE_INFINITY };
    for (let y = 0; y < canvas.height; y += 1) {
      for (let x = 0; x < canvas.width; x += 1) {
        const index = (y * canvas.width + x) * 4;
        const red = data[index] ?? 0;
        const green = data[index + 1] ?? 0;
        const blue = data[index + 2] ?? 0;
        if (red > 100 && red > green * 1.35 && red > blue * 1.5) {
          count += 1;
          const distance = (x - canvas.width / 2) ** 2 + (y - canvas.height / 2) ** 2;
          if (distance < nearest.distance) nearest = { x, y, distance };
        }
      }
    }
    return { count, nearest };
  }, png.toString('base64'));
}

test('FIRMS fixtures draw orange fire cells, can be picked and expose their source and legend', async ({
  page,
  request,
  baseURL,
}, testInfo) => {
  const { catalog, layers } = await availableLayers(request, ['fires']);
  const fireLayer = layers.fires.name;
  const problems = await watchPage(page);
  await page.goto(workspaceShareUrl(view, baseURL ?? 'http://127.0.0.1:7412/'));
  await expect(page.getByText('Loading globe…')).toBeHidden({ timeout: 30_000 });
  await expect(page.getByText('Camera at 400 km', { exact: true })).toBeVisible();
  const fires = page.getByRole('switch', { name: fireLayer, exact: true });
  await expect(fires).not.toBeChecked();
  const unlit = (await orangePixels(page)).count;

  // First enable in this fresh browser workspace, with the real fixture HTTP/WS path.
  // The time includes click, feed delivery, rendering and screenshot readback. It is recorded,
  // not asserted: under a parallel suite each screenshot sample alone can take ~1 s. The
  // ≤ 3 s activation target (ENG-950) is measured by the performance harness
  // (`scripts/perf.sh browser`, docs/performance/), under controlled conditions.
  const started = performance.now();
  await fires.click();
  await expect
    .poll(async () => (await orangePixels(page)).count, { timeout: 15_000, intervals: [50, 100] })
    .toBeGreaterThan(unlit + 100);
  const toggleToDrawnMs = performance.now() - started;
  await testInfo.attach('fire-fixture-timing.json', {
    body: JSON.stringify({
      toggleToDrawnMs,
      detections: 8,
      workload: 'synthetic Helsinki viewport; fresh browser workspace; shared fixture server',
      includes: 'toggle action, feed delivery, rendering and screenshot readback',
      scope: 'local acceptance only; not a live or worldwide performance claim',
    }),
    contentType: 'application/json',
  });

  const response = await request.get(`/api/v1/feeds/fires?bbox=${HELSINKI_BBOX}`);
  expect(response.ok()).toBe(true);
  const snapshot = await response.json();
  expect(snapshot.feed).toBe('fires');
  expect(snapshot.items.kind).toBe('fires');
  const cells: FireCell[] = snapshot.items.items;
  expect(cells).toHaveLength(8);
  expect(cells.every((cell) => cell.count === 1)).toBe(true);
  expect(cells).toContainEqual(expect.objectContaining({ lat: 60.17, lon: 24.94, frp: 80 }));
  expect(snapshot.notices).toContain('Recorded fixture data, not live.');

  const painted = await orangePixels(page);
  expect(painted.nearest.distance).toBeLessThan(1_000);
  await page.locator(GLOBE_CANVAS).click({
    position: { x: painted.nearest.x, y: painted.nearest.y },
  });
  const inspector = page.getByRole('region', { name: 'Active fire detection', exact: true });
  await expect(inspector).toBeVisible();
  await expect(inspector.getByText('80.0 MW', { exact: true })).toBeVisible();
  await expect(inspector.getByText('340.0 K', { exact: true })).toBeVisible();
  await expect(inspector.getByText('TEST-N20', { exact: true })).toBeVisible();
  await expect(inspector.getByText('Night', { exact: true })).toBeVisible();
  await expect(
    inspector.getByText(sourceName(catalog, 'nasa-firms'), { exact: true }),
  ).toBeVisible();
  await page.getByRole('button', { name: `${fireLayer} options`, exact: true }).click();
  const legend = page.getByRole('list', { name: 'Active fire intensity legend' });
  await expect(legend).toBeVisible();
  await expect(legend.getByText('50–<100 MW', { exact: true })).toBeVisible();
  await page.screenshot({
    path: testInfo.outputPath('fire-inspector-and-legend.png'),
    fullPage: true,
  });
  expect(problems.errors).toEqual([]);
  expect(await problems.cspViolations()).toEqual([]);

  await fires.click();
  await expect(inspector).toBeHidden();
  await expect.poll(async () => (await orangePixels(page)).count).toBe(unlit);
});

for (const [imageryId, layer] of [
  ['gibs-blue-marble', 'BlueMarble_ShadedRelief_Bathymetry'],
  ['gibs-black-marble', 'VIIRS_Black_Marble'],
] as const) {
  test(`optional real NASA GIBS ${imageryId} scene preserves fire visibility`, async ({
    page,
    request,
    baseURL,
  }, testInfo) => {
    test.skip(
      process.env.LAVENDER_E2E_LIVE_IMAGERY !== '1',
      'Explicit network imagery acceptance run.',
    );
    test.setTimeout(150_000);
    const { layers } = await availableLayers(request, ['fires']);
    const fireLayer = layers.fires.name;
    const problems = await watchPage(page, { liveImagery: true });
    const tiles = recordGibsTiles(page);
    await page.goto(workspaceShareUrl({ ...view, imageryId }, baseURL ?? 'http://127.0.0.1:7412/'));
    await expect(page.getByText('Loading globe…')).toBeHidden({ timeout: 30_000 });
    await gibsTilesSettled(page, tiles, layer);

    // Real imagery has orange-ish pixels of its own (city lights, terrain): count the fires as
    // the gain over the same view with them off, measured before and after.
    const fires = page.getByRole('switch', { name: fireLayer, exact: true });
    const before = (await orangePixels(page)).count;
    await fires.click();
    await expect
      .poll(async () => (await orangePixels(page)).count, { timeout: 10_000 })
      .toBeGreaterThan(before + 100);
    const on = (await orangePixels(page)).count;
    await fires.click();
    await expect.poll(async () => (await orangePixels(page)).count).toBeLessThan(before + 50);
    const after = (await orangePixels(page)).count;
    expect(on - Math.max(before, after), `fires on ${on}, off ${before}/${after}`).toBeGreaterThan(
      100,
    );

    await fires.click();
    await page.getByRole('button', { name: `${fireLayer} options`, exact: true }).click();
    await expect(page.getByRole('list', { name: 'Active fire intensity legend' })).toBeVisible();
    await page.screenshot({ path: testInfo.outputPath(`${imageryId}-fires.png`), fullPage: true });
    const own = tiles.filter((tile) => tile.url.includes(layer));
    await testInfo.attach('real-gibs-tile-responses.json', {
      body: JSON.stringify({ before, on, after, tiles: own }),
      contentType: 'application/json',
    });
    expect(own.every((tile) => tile.status === 200 && tile.contentType.startsWith('image/'))).toBe(
      true,
    );
    expect(problems.errors).toEqual([]);
    expect(await problems.cspViolations()).toEqual([]);
  });
}
