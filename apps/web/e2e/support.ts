import { type APIRequestContext, expect, type Locator, type Page } from '@playwright/test';
import type { Catalog, FeedId, LayerDescriptor } from '../src/contracts';

type CspWindow = Window & { __cspViolations?: string[] };

/** The globe's WebGL canvas. */
export const GLOBE_CANVAS = '.lv-globe-host canvas';

/** A 1×1 transparent PNG: stands in for remote imagery tiles so runs are hermetic. */
const BLANK_TILE = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==',
  'base64',
);

export interface PageProblems {
  readonly errors: string[];
  cspViolations(): Promise<string[]>;
}

/**
 * Records console errors, page errors and CSP violations, and answers remote imagery tile
 * requests locally (the catalog's tile hosts stay allowed by the CSP; only the network is
 * avoided). Explicit live-imagery acceptance runs can opt out of the tile stub.
 * Call before `page.goto`.
 */
export async function watchPage(
  page: Page,
  options: { readonly liveImagery?: boolean } = {},
): Promise<PageProblems> {
  const errors: string[] = [];
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text());
  });
  page.on('pageerror', (error) => errors.push(error.message));
  await page.addInitScript(() => {
    const target = window as CspWindow;
    target.__cspViolations = [];
    document.addEventListener('securitypolicyviolation', (event) => {
      target.__cspViolations?.push(`${event.violatedDirective} ${event.blockedURI || 'inline'}`);
    });
  });
  if (!options.liveImagery) {
    await page.route('https://gibs.earthdata.nasa.gov/**', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'image/png',
        headers: { 'Access-Control-Allow-Origin': '*' },
        body: BLANK_TILE,
      }),
    );
  }
  return {
    errors,
    cspViolations: () => page.evaluate(() => (window as CspWindow).__cspViolations ?? []),
  };
}

/**
 * Share of lit pixels (r+g+b > 60) in a screenshot of `selector`. The PNG is decoded by the
 * page itself (the CSP allows `data:` images), so no image library is needed here.
 */
export async function litRatio(page: Page, selector: string): Promise<number> {
  const png = await page.locator(selector).screenshot();
  return page.evaluate(async (b64) => {
    const image = new Image();
    image.src = `data:image/png;base64,${b64}`;
    await image.decode();
    const canvas = document.createElement('canvas');
    canvas.width = image.naturalWidth;
    canvas.height = image.naturalHeight;
    const ctx = canvas.getContext('2d');
    if (!ctx) return 0;
    ctx.drawImage(image, 0, 0);
    const { data } = ctx.getImageData(0, 0, canvas.width, canvas.height);
    let lit = 0;
    for (let i = 0; i < data.length; i += 4) {
      if ((data[i] ?? 0) + (data[i + 1] ?? 0) + (data[i + 2] ?? 0) > 60) lit += 1;
    }
    return lit / (data.length / 4);
  }, png.toString('base64'));
}

/**
 * The catalog and its descriptors for `feeds`, failing unless every one of those layers is
 * available. Tests take layer names from the server instead of hard-coding them. Fixture mode
 * replays recorded documents, so a layer that is not available here is a server
 * configuration problem, and the failure says which layer and why.
 */
export async function availableLayers<F extends FeedId>(
  request: APIRequestContext,
  feeds: readonly F[],
): Promise<{ readonly catalog: Catalog; readonly layers: Readonly<Record<F, LayerDescriptor>> }> {
  const response = await request.get('/api/v1/catalog');
  expect(response.ok(), 'GET /api/v1/catalog').toBe(true);
  const catalog = (await response.json()) as Catalog;
  const entries = feeds.map((feed) => {
    const layer = catalog.layers.find((l) => l.feed === feed);
    if (!layer) throw new Error(`The server catalog has no ${feed} layer.`);
    expect(
      layer.availability,
      `the ${feed} layer ("${layer.name}") must be available in fixture mode`,
    ).toEqual({ state: 'available' });
    return [feed, layer] as const;
  });
  return { catalog, layers: Object.fromEntries(entries) as Record<F, LayerDescriptor> };
}

/** A source's display name from the catalog (what the inspector shows as the source). */
export function sourceName(catalog: Catalog, id: string): string {
  const source = catalog.sources.find((s) => s.id === id);
  if (!source) throw new Error(`The server catalog has no ${id} source.`);
  return source.name;
}

export interface TileResponse {
  readonly url: string;
  readonly status: number;
  readonly contentType: string;
  readonly bytes: number;
}

/** Records every NASA GIBS tile response the page receives. Call before `page.goto`. */
export function recordGibsTiles(page: Page): TileResponse[] {
  const tiles: TileResponse[] = [];
  page.on('response', async (response) => {
    if (!response.url().startsWith('https://gibs.earthdata.nasa.gov/')) return;
    const body = await response.body().catch(() => Buffer.alloc(0));
    tiles.push({
      url: response.url(),
      status: response.status(),
      contentType: response.headers()['content-type'] ?? '',
      bytes: body.length,
    });
  });
  return tiles;
}

/** Real tiles only: waits until `layer` tiles have loaded and stopped arriving. */
export async function gibsTilesSettled(
  page: Page,
  tiles: readonly TileResponse[],
  layer: string,
): Promise<void> {
  const loaded = () => tiles.filter((t) => t.url.includes(layer) && t.bytes > 1_000).length;
  await expect.poll(loaded, { timeout: 60_000 }).toBeGreaterThan(0);
  let previous = -1;
  await expect
    .poll(
      () => {
        const now = loaded();
        const stable = now === previous;
        previous = now;
        return stable;
      },
      { timeout: 60_000, intervals: [1_500] },
    )
    .toBe(true);
  await page.waitForTimeout(500); // the frame after the last tile
}

/**
 * Panels that float over the globe canvas. Their hazard-family icons and legend swatches use
 * the symbol colours, so pixel counts paint them out (see `canvasScreenshot`).
 */
export function canvasOverlays(page: Page): Locator[] {
  return [page.getByRole('region', { name: 'Layers', exact: true })];
}

/** A screenshot of `selector` with the floating panels over it painted black. */
export function canvasScreenshot(page: Page, selector = GLOBE_CANVAS): Promise<Buffer> {
  return page.locator(selector).screenshot({ mask: canvasOverlays(page), maskColor: '#000000' });
}

/** The validated hazard ramp, low → high (docs/design/design-system.md §4). */
export const HAZARD_RAMP = ['#8f3c19', '#b9471d', '#d95926', '#ef7a45', '#f8a57a'] as const;

export interface RampScan {
  /** Pixels carrying each ramp colour, low → high. */
  readonly counts: number[];
  /** Per ramp colour, the matching pixel nearest the element's centre, in CSS pixels. */
  readonly nearest: ({ readonly x: number; readonly y: number } | null)[];
}

/**
 * Scans a screenshot of `selector` for the hazard ramp colours (every channel within
 * `tolerance`), with the floating panels painted out. Symbol interiors are drawn in the exact
 * ramp colour; their anti-aliased rims and the imagery are mostly excluded, so compare against
 * the same view with the layers off rather than against zero when real imagery is shown.
 * `colours` scans for other exact symbol colours instead (counts and nearest in that order).
 */
export async function scanRamp(
  page: Page,
  selector = GLOBE_CANVAS,
  tolerance = 10,
  colours: readonly string[] = HAZARD_RAMP,
): Promise<RampScan> {
  const png = await canvasScreenshot(page, selector);
  const box = await page.locator(selector).boundingBox();
  return page.evaluate(
    async ({ b64, ramp, tolerance, cssWidth }) => {
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
      const colours = ramp.map((hex) =>
        [1, 3, 5].map((at) => Number.parseInt(hex.slice(at, at + 2), 16)),
      );
      const scale = cssWidth > 0 ? canvas.width / cssWidth : 1;
      const counts = ramp.map(() => 0);
      const best = ramp.map(() => ({ x: 0, y: 0, distance: Number.POSITIVE_INFINITY }));
      for (let i = 0; i < data.length; i += 4) {
        const step = colours.findIndex(
          ([r = 0, g = 0, b = 0]) =>
            Math.abs((data[i] ?? 0) - r) <= tolerance &&
            Math.abs((data[i + 1] ?? 0) - g) <= tolerance &&
            Math.abs((data[i + 2] ?? 0) - b) <= tolerance,
        );
        if (step < 0) continue;
        counts[step] = (counts[step] ?? 0) + 1;
        const x = (i / 4) % canvas.width;
        const y = Math.floor(i / 4 / canvas.width);
        const distance = (x - canvas.width / 2) ** 2 + (y - canvas.height / 2) ** 2;
        const current = best[step];
        if (current && distance < current.distance) best[step] = { x, y, distance };
      }
      return {
        counts,
        nearest: best.map(({ x, y, distance }) =>
          Number.isFinite(distance) ? { x: x / scale, y: y / scale } : null,
        ),
      };
    },
    {
      b64: png.toString('base64'),
      ramp: [...colours],
      tolerance,
      cssWidth: box?.width ?? 0,
    },
  );
}

/** Pixels of `selector` carrying each hazard ramp colour, low → high (see `scanRamp`). */
export async function rampPixels(page: Page, selector?: string): Promise<number[]> {
  return (await scanRamp(page, selector)).counts;
}
