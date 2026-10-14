import { expect, test } from '@playwright/test';
import { watchPage } from './support';

test('predict geometric satellite passes through the real Rust API from the inspector', async ({
  page,
}, testInfo) => {
  const problems = await watchPage(page);
  await page.goto('/');
  await expect(page.getByText('Loading globe…')).toBeHidden({ timeout: 30_000 });
  await page.getByRole('switch', { name: 'Satellites', exact: true }).click();
  const search = page.getByRole('combobox', { name: 'Search places, coordinates and callsigns' });
  await search.click();
  await search.fill('25544');
  await expect(page.getByRole('option', { name: /^TEST STATION / })).toBeVisible({
    timeout: 30_000,
  });
  await search.press('Enter');
  const inspector = page.getByRole('region', { name: 'TEST STATION', exact: true });
  await expect(inspector).toBeVisible();
  const response = page.waitForResponse((reply) =>
    reply.url().includes('/satellites/25544/passes'),
  );
  await inspector.getByRole('button', { name: 'Predict passes at map centre' }).click();
  const prediction = await response;
  expect(prediction.status()).toBe(200);
  const body = await prediction.json();
  expect(body.noradId).toBe(25544);
  expect(body.observer.altM).toBe(0);
  expect(body.passes.length).toBeLessThanOrEqual(64);
  await expect(
    inspector.getByText(`${body.passes.length} predicted passes`, { exact: true }),
  ).toBeVisible();
  await expect(inspector.getByText(/Geometric passes at or above 0°/)).toBeVisible();
  await expect(inspector.getByText(/Observer:/)).toBeVisible();
  expect(problems.errors).toEqual([]);
  expect(await problems.cspViolations()).toEqual([]);
  await page.screenshot({ path: testInfo.outputPath('satellite-passes.png'), fullPage: true });
});
