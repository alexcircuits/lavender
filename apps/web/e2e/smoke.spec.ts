import { expect, test } from '@playwright/test';
import { litRatio, watchPage } from './support';

test('the shell paints, connects, streams the default layer and starts the globe cleanly', async ({
  page,
}) => {
  const problems = await watchPage(page);
  await page.goto('/');

  // The shell paints before the engine chunk arrives.
  await expect(page.getByRole('main', { name: 'Globe' })).toBeVisible();
  await expect(page.getByText(/\d{2}:\d{2}:\d{2} UTC/)).toBeVisible();

  // The platform connects and the default layer (earthquakes) streams the recorded feed.
  await expect(page.getByText('Connected', { exact: true })).toBeVisible({ timeout: 30_000 });
  const quakes = page
    .getByRole('listitem')
    .filter({ has: page.getByRole('switch', { name: 'Earthquakes' }) });
  await expect(page.getByRole('switch', { name: 'Earthquakes' })).toBeChecked();
  await expect(quakes).toContainText('45', { timeout: 30_000 });

  // The lazily loaded engine mounts one canvas.
  await expect(page.getByText('Loading globe…')).toBeHidden({ timeout: 30_000 });
  await expect(page.getByText('The globe could not start.', { exact: false })).toHaveCount(0);
  await expect(page.locator('.lv-globe-host canvas')).toHaveCount(1);

  // Let frames and imagery settle, then check the globe actually drew something: the home
  // view's globe fills well over a tenth of the canvas; a blank or frozen canvas is ~0.
  await page.waitForTimeout(2000);
  expect(await litRatio(page, '.lv-globe-host canvas')).toBeGreaterThan(0.1);
  expect(problems.errors).toEqual([]);
  expect(await problems.cspViolations()).toEqual([]);
});

test('an idle globe draws nothing (request-render mode)', async ({ page }) => {
  // Count WebGL draw calls from the first frame on.
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
  await watchPage(page);
  await page.goto('/');
  const quakes = page
    .getByRole('listitem')
    .filter({ has: page.getByRole('switch', { name: 'Earthquakes' }) });
  await expect(quakes).toContainText('45', { timeout: 30_000 });
  const draws = () => page.evaluate(() => (window as Window & { __draws?: number }).__draws ?? 0);
  // Let imagery load and the swap finish, then watch an idle scene for two seconds.
  await page.waitForTimeout(4000);
  expect(await draws()).toBeGreaterThan(0);
  const before = await draws();
  await page.waitForTimeout(2000);
  expect(await draws()).toBe(before);
});

test('the server sends the security headers and the content security policy', async ({
  request,
}) => {
  const response = await request.get('/');
  expect(response.status()).toBe(200);
  const csp = response.headers()['content-security-policy'] ?? '';
  expect(csp).toContain("script-src 'self' 'wasm-unsafe-eval'");
  expect(csp).not.toContain("'unsafe-eval'");
  expect(csp).toContain('gibs.earthdata.nasa.gov');
  expect(response.headers()['x-content-type-options']).toBe('nosniff');
});
