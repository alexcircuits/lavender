import { expect, test } from '@playwright/test';
import { watchPage } from './support';

const isMac = process.platform === 'darwin';

test('turn on aircraft, find a callsign, inspect it, and go to coordinates', async ({ page }) => {
  const problems = await watchPage(page);
  await page.goto('/');
  await expect(page.getByText('Connected', { exact: true })).toBeVisible({ timeout: 30_000 });
  await expect(page.getByText('Loading globe…')).toBeHidden({ timeout: 30_000 });

  // Turning a layer on opens its subscription; the recorded feed has six valid contacts
  // and one malformed row the Rust parser skips.
  const aircraft = page.getByRole('switch', { name: 'Aircraft' });
  await aircraft.click();
  await expect(aircraft).toBeChecked();
  const row = page.getByRole('listitem').filter({ has: aircraft });
  await expect(row).toContainText('6', { timeout: 30_000 });

  // Search finds the live entity and selecting it opens the inspector with provenance.
  const search = page.getByRole('combobox', { name: 'Search places, coordinates and callsigns' });
  await search.click();
  await search.fill('FIN7KV');
  const hit = page.getByRole('option', { name: /^FIN7KV / });
  await expect(hit).toBeVisible();
  await search.press('Enter');
  const inspector = page.getByRole('region', { name: 'FIN7KV' });
  await expect(inspector).toBeVisible();
  await expect(inspector).toContainText('OH-LWA');
  await expect(inspector).toContainText('35VLG');
  await expect(inspector).toContainText('adsb.lol');

  // History is captured by the same shared Rust refresh.
  await inspector.getByRole('button', { name: 'Load recent history' }).click();
  await expect(inspector.getByRole('table', { name: 'Aircraft history' })).toBeVisible();
  await expect(inspector).toContainText('retained observations · adsb.lol');

  // Escape clears the selection (from the inspector; in the search field it belongs to search).
  await inspector.getByRole('button', { name: 'Fly to' }).focus();
  await page.keyboard.press('Escape');
  await expect(inspector).toBeHidden();

  // The command palette resolves coordinates without a network call.
  await page.keyboard.press(isMac ? 'Meta+k' : 'Control+k');
  const palette = page.getByRole('dialog', { name: 'Search and commands' });
  await expect(palette).toBeVisible();
  await page.keyboard.type('60.17, 24.94');
  await expect(palette.getByRole('option').first()).toHaveText(/Go to 60\.1700° N, 24\.9400° E/);
  await page.keyboard.press('Enter');
  await expect(palette).toBeHidden();

  await page.waitForTimeout(1500);
  expect(problems.errors).toEqual([]);
  expect(await problems.cspViolations()).toEqual([]);
});
