import type { Page } from '@playwright/test';

type CspWindow = Window & { __cspViolations?: string[] };

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
 * avoided). Call before `page.goto`.
 */
export async function watchPage(page: Page): Promise<PageProblems> {
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
  await page.route('https://gibs.earthdata.nasa.gov/**', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'image/png',
      headers: { 'Access-Control-Allow-Origin': '*' },
      body: BLANK_TILE,
    }),
  );
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
