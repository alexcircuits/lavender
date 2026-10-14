import { expect, test } from '@playwright/test';
import {
  MAX_SHARE_URL_LENGTH,
  type WorkspaceDocument,
  workspaceDocumentFromUrl,
  workspaceShareUrl,
} from '../src/state/workspaceDocument';
import { watchPage } from './support';

const shared: WorkspaceDocument = {
  schemaVersion: 2,
  camera: {
    lon: 24.94,
    lat: 60.17,
    heightM: 400_000,
    headingRad: 0.3,
    pitchRad: -1.2,
    rollRad: 0.1,
  },
  layers: [
    { id: 'aircraft', enabled: true, options: { militaryOnly: false } },
    { id: 'earthquakes', enabled: false, options: { minMagnitude: 4.5 } },
    { id: 'ships', enabled: true, options: {} },
  ],
  imageryId: 'natural-earth-ii',
  selection: { feed: 'aircraft', id: '461f2a' },
  annotations: [],
};

type ClipboardWindow = Window & { __copiedView?: string };

test('a shared fixture view restores and the toolbar copies its current camera and options', async ({
  page,
  baseURL,
}, testInfo) => {
  const problems = await watchPage(page);
  await page.addInitScript(() => {
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: {
        writeText: async (value: string) => {
          (window as ClipboardWindow).__copiedView = value;
        },
      },
    });
  });
  await page.goto(workspaceShareUrl(shared, baseURL ?? 'http://127.0.0.1:7412/'));
  await expect(page.getByText('Loading globe…')).toBeHidden({ timeout: 30_000 });
  await expect(page.getByRole('switch', { name: 'Aircraft', exact: true })).toBeChecked();
  await expect(page.getByRole('switch', { name: 'Earthquakes', exact: true })).not.toBeChecked();
  await expect(page.getByRole('region', { name: 'FIN7KV' })).toBeVisible({ timeout: 30_000 });
  await page.getByRole('button', { name: 'Aircraft options', exact: true }).click();
  await expect(page.getByRole('switch', { name: 'Military only' })).not.toBeChecked();
  await page.getByRole('button', { name: 'Copy view link', exact: true }).click();
  await expect(page.getByText('Link copied', { exact: true })).toBeVisible();
  const copied = await page.evaluate(() => (window as ClipboardWindow).__copiedView);
  expect(copied).toBeDefined();
  const restored = workspaceDocumentFromUrl(copied ?? '');
  expect(restored?.selection).toEqual(shared.selection);
  expect(restored?.imageryId).toBe('natural-earth-ii');
  expect(restored?.layers).toContainEqual(shared.layers[0]);
  expect(restored?.layers.some((layer) => layer.id === 'ships')).toBe(false);
  for (const key of ['lon', 'lat', 'heightM', 'headingRad', 'pitchRad', 'rollRad'] as const) {
    expect(restored?.camera[key]).toBeCloseTo(shared.camera[key], key === 'heightM' ? 2 : 5);
  }
  expect(problems.errors).toEqual([]);
  expect(await problems.cspViolations()).toEqual([]);
  await page.screenshot({ path: testInfo.outputPath('restored-workspace.png'), fullPage: true });
});

test('hash navigation and browser back/forward restore different views without remounting', async ({
  page,
  baseURL,
}) => {
  const problems = await watchPage(page);
  await page.addInitScript(() => {
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: {
        writeText: async (value: string) => {
          (window as ClipboardWindow).__copiedView = value;
        },
      },
    });
  });
  const expectCamera = async (expected: WorkspaceDocument['camera']) => {
    await page.getByRole('button', { name: 'Copy view link', exact: true }).click();
    const copied = await page.evaluate(() => (window as ClipboardWindow).__copiedView);
    const document = workspaceDocumentFromUrl(copied ?? '');
    for (const key of ['lon', 'lat', 'heightM', 'headingRad', 'pitchRad', 'rollRad'] as const) {
      expect(document?.camera[key]).toBeCloseTo(expected[key], key === 'heightM' ? 2 : 5);
    }
  };
  await page.goto(workspaceShareUrl(shared, baseURL ?? 'http://127.0.0.1:7412/'));
  await expect(page.getByRole('region', { name: 'FIN7KV' })).toBeVisible({ timeout: 30_000 });
  const second: WorkspaceDocument = {
    ...shared,
    camera: { ...shared.camera, lon: -120, lat: 20, heightM: 1_000_000 },
    layers: [{ id: 'earthquakes', enabled: true, options: { minMagnitude: 4.5 } }],
    selection: null,
  };
  const next = workspaceShareUrl(second, baseURL ?? 'http://127.0.0.1:7412/');
  await page.evaluate((url) => {
    window.location.hash = new URL(url).hash;
  }, next);
  await expect(page.getByRole('switch', { name: 'Earthquakes', exact: true })).toBeChecked();
  await expect(page.getByRole('switch', { name: 'Aircraft', exact: true })).not.toBeChecked();
  await expect(page.getByRole('region', { name: 'FIN7KV' })).toBeHidden();
  await expectCamera(second.camera);
  await page.goBack();
  await expect(page.getByRole('switch', { name: 'Aircraft', exact: true })).toBeChecked();
  await expect(page.getByRole('region', { name: 'FIN7KV' })).toBeVisible();
  await expectCamera(shared.camera);
  await page.goForward();
  await expect(page.getByRole('switch', { name: 'Earthquakes', exact: true })).toBeChecked();
  await expect(page.getByRole('region', { name: 'FIN7KV' })).toBeHidden();
  await expectCamera(second.camera);
  expect(problems.errors).toEqual([]);
  expect(await problems.cspViolations()).toEqual([]);
});

test('invalid and oversized links show a short message while the workspace stays usable', async ({
  page,
}) => {
  const problems = await watchPage(page);
  await page.goto('/#view=broken!');
  await expect(page.getByText('Could not open shared view', { exact: true }).last()).toBeVisible();
  await expect(page.getByText('Loading globe…')).toBeHidden({ timeout: 30_000 });
  const aircraft = page.getByRole('switch', { name: 'Aircraft', exact: true });
  await aircraft.click();
  await expect(aircraft).toBeChecked();
  await page.goto(`/#view=${'a'.repeat(MAX_SHARE_URL_LENGTH)}`);
  await expect(page.getByText('Could not open shared view', { exact: true }).last()).toBeVisible();
  await expect(page.getByText('Loading globe…')).toBeHidden({ timeout: 30_000 });
  await expect(page.getByRole('button', { name: 'Copy view link', exact: true })).toBeEnabled();
  expect(problems.errors).toEqual([]);
  expect(await problems.cspViolations()).toEqual([]);
});
