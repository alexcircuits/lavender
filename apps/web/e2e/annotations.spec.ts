import { expect, test } from '@playwright/test';
import { workspaceDocumentFromUrl, workspaceShareUrl } from '../src/state/workspaceDocument';
import { watchPage } from './support';

type ClipboardWindow = Window & { __annotationLink?: string };
test('pins, distance lines and areas survive edit, reload and shared workspace restoration', async ({
  page,
  baseURL,
}, testInfo) => {
  const problems = await watchPage(page);
  await page.addInitScript(() => {
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: {
        writeText: async (value: string) => {
          (window as ClipboardWindow).__annotationLink = value;
        },
      },
    });
  });
  await page.goto('/');
  await expect(page.getByText('Loading globe…')).toBeHidden({ timeout: 30_000 });
  await page.getByRole('button', { name: 'Annotations', exact: true }).click();
  await page.getByRole('button', { name: 'New pin' }).click();
  await page.getByLabel('Label', { exact: true }).fill('Field pin');
  const canvas = page.locator('.lv-globe-host canvas').first();
  const bounds = await canvas.boundingBox();
  if (!bounds) throw new Error('Globe canvas missing.');
  await canvas.click({ position: { x: bounds.width * 0.6, y: bounds.height * 0.5 } });
  await expect(page.getByRole('button', { name: 'Save annotation' })).toBeEnabled();
  await expect(page.getByRole('main', { name: 'Globe' })).toBeFocused();
  await page.getByLabel('Label', { exact: true }).click();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('heading', { name: 'Draw pin' })).toBeVisible();
  await canvas.click({ position: { x: bounds.width * 0.6, y: bounds.height * 0.5 } });
  await expect(page.getByRole('main', { name: 'Globe' })).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('heading', { name: 'Field pin' })).toBeVisible();

  await page.getByRole('button', { name: 'New line' }).click();
  await page.getByLabel('Label', { exact: true }).fill('Distance arrow');
  await page.getByLabel(/Vertices/).fill('35, -96\n35, -94');
  await page.getByRole('button', { name: 'Apply vertices' }).click();
  await page.getByRole('button', { name: 'Save annotation' }).click();
  await expect(page.getByRole('heading', { name: 'Distance arrow' })).toBeVisible();

  await page.getByRole('button', { name: 'New area' }).click();
  await page.getByLabel('Label', { exact: true }).fill('Study area');
  await page.getByLabel(/Vertices/).fill('34.5, -95.5\n34.5, -94.5\n35.5, -94.5\n35.5, -95.5');
  await page.getByRole('button', { name: 'Apply vertices' }).click();
  await page.getByRole('button', { name: 'Save annotation' }).click();
  await page.getByRole('button', { name: 'Edit Study area' }).click();
  await page.getByLabel('Note', { exact: true }).fill('User defined boundary');
  await page.getByRole('button', { name: 'Save annotation' }).click();
  // Reload immediately: durable marks must not depend on a quiet-period timer.
  await page.reload();
  await expect(page.getByText('Loading globe…')).toBeHidden({ timeout: 30_000 });
  await page.getByRole('button', { name: 'Annotations', exact: true }).click();
  await expect(page.getByText('3 / 24 saved annotations')).toBeVisible();
  await expect(page.getByText('User defined boundary')).toBeVisible();
  await page.getByRole('button', { name: 'Copy view link', exact: true }).click();
  const copied = await page.evaluate(() => (window as ClipboardWindow).__annotationLink);
  const document = workspaceDocumentFromUrl(copied ?? '');
  expect(document?.annotations).toHaveLength(3);
  if (!document) throw new Error('Missing shared document.');
  const detailed = {
    ...document,
    camera: {
      ...document.camera,
      lat: 35,
      lon: -95,
      heightM: 1_000_000,
      headingRad: 0,
      pitchRad: -Math.PI / 2,
      rollRad: 0,
    },
  };
  await page.evaluate(() => localStorage.clear());
  await page.goto(workspaceShareUrl(detailed, baseURL ?? 'http://127.0.0.1:7412'));
  await page.evaluate(() => localStorage.clear());
  await page.reload(); // Hash navigation alone would keep this runtime and its open panel.
  await expect(page.getByText('Loading globe…')).toBeHidden({ timeout: 30_000 });
  await page.getByRole('button', { name: 'Annotations', exact: true }).click();
  await expect(page.getByText('3 / 24 saved annotations')).toBeVisible();
  await expect(page.getByText('User defined boundary')).toBeVisible();
  await page.screenshot({
    path: testInfo.outputPath('persistent-annotations.png'),
    fullPage: true,
  });
  await page.getByRole('button', { name: 'Delete Field pin' }).click();
  await expect(page.getByText('2 / 24 saved annotations')).toBeVisible();
  expect(problems.errors).toEqual([]);
  expect(await problems.cspViolations()).toEqual([]);
});
