import { expect, test } from '@playwright/test';
import { watchPage } from './support';

test('documentation navigation, source reading and diagram work under production CSP', async ({
  page,
}) => {
  const problems = await watchPage(page);
  await page.setViewportSize({ width: 1440, height: 1000 });
  const requests: string[] = [];
  page.on('request', (request) => requests.push(request.url()));
  await page.goto('/docs/');
  await expect(page.getByRole('heading', { name: 'Build with Lavender.' })).toBeVisible();
  await page.evaluate(() => document.fonts.ready);
  const desktopScreenshot = test.info().outputPath('docs-desktop.png');
  await page.screenshot({ path: desktopScreenshot });
  await test
    .info()
    .attach('Desktop documentation', { path: desktopScreenshot, contentType: 'image/png' });
  expect(requests.some((url) => /\/api\/|GlobeEngine|\/cesium\//.test(url))).toBe(false);
  await page.getByRole('link', { name: 'Explore architecture' }).click();
  await expect(page.getByRole('heading', { name: 'Architecture', exact: true })).toBeVisible();
  await expect(page.getByRole('img', { name: 'Lavender architecture diagram' })).toBeVisible();
  await page.getByRole('link', { name: 'hub/slots.rs', exact: true }).click();
  const source = page.getByRole('dialog', { name: 'Repository source' });
  await expect(
    source.getByText('crates/lavender-core/src/hub/slots.rs', { exact: true }),
  ).toBeVisible();
  await expect(source.locator('code')).toContainText('impl RegionBudget');
  await page.keyboard.press('Escape');
  await expect(source).not.toBeVisible();
  await page
    .getByRole('heading', { name: 'An aircraft record end to end' })
    .getByRole('link')
    .click();
  await expect(page).toHaveURL(/#\/architecture#an-aircraft-record-end-to-end$/);
  await page.reload();
  await expect(
    page.getByRole('heading', { name: 'An aircraft record end to end' }),
  ).toBeInViewport();
  await page.getByRole('button', { name: 'View Markdown' }).click();
  await expect(source.locator('code')).toContainText('# Architecture Explained Simply');
  await page.keyboard.press('Escape');
  await page.goBack();
  await expect(page).toHaveURL(/#\/architecture$/);
  expect(problems.errors).toEqual([]);
  expect(await problems.cspViolations()).toEqual([]);
});

test('keyboard search, code copying and unknown-page recovery', async ({ page, context }) => {
  const problems = await watchPage(page);
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.goto('/docs/');
  await page.getByRole('button', { name: 'Copy From the Lavender repository root code' }).click();
  await expect(
    page.getByRole('button', { name: 'Copy From the Lavender repository root code' }),
  ).toHaveText('Copied');
  expect(await page.evaluate(() => navigator.clipboard.readText())).toContain(
    'cargo build --locked -p lavender-server',
  );
  await page.keyboard.press('ControlOrMeta+k');
  const input = page.getByRole('combobox', { name: 'Search documentation' });
  await expect(input).toBeFocused();
  await input.fill('fixtures');
  const results = page.getByRole('listbox');
  await expect(results.getByRole('option').filter({ hasText: 'Run locally' })).toBeVisible();
  await expect(page.getByText('Loading full-text search…', { exact: true })).not.toBeVisible();
  await expect(
    page.getByText('Some guides could not load. Available pages are searchable.', { exact: true }),
  ).not.toBeVisible();
  await input.fill('Rust rewrite');
  await expect(results.getByRole('option').first()).toContainText('Rust rewrite');
  await input.press('Enter');
  await expect(page).toHaveURL(/#\/rust-rewrite$/);
  await expect(page.getByRole('heading', { name: 'Rust rewrite', exact: true })).toBeVisible();
  await page.goto('/docs/#/unknown-guide');
  await expect(page.getByRole('heading', { name: 'This guide could not be found.' })).toBeVisible();
  await page.getByRole('link', { name: 'Back to overview' }).click();
  await expect(page.getByRole('heading', { name: 'Build with Lavender.' })).toBeVisible();
  expect(problems.errors).toEqual([]);
  expect(await problems.cspViolations()).toEqual([]);
});

test('phone navigation, focus restoration and readable wide content', async ({ page }) => {
  const problems = await watchPage(page);
  await page.setViewportSize({ width: 360, height: 800 });
  await page.goto('/docs/');
  const menu = page.getByRole('button', { name: 'Open documentation navigation' });
  await menu.click();
  await page
    .getByRole('dialog', { name: 'Documentation navigation' })
    .getByRole('link', { name: 'Run locally', exact: true })
    .click();
  await expect(page.getByRole('heading', { name: 'Run locally', exact: true })).toBeVisible();
  await expect(page.getByRole('dialog')).not.toBeVisible();
  await expect(page.getByRole('heading', { name: 'Prerequisites and install' })).toBeVisible();
  const phoneScreenshot = test.info().outputPath('docs-phone.png');
  await page.screenshot({ path: phoneScreenshot });
  await test
    .info()
    .attach('Phone documentation', { path: phoneScreenshot, contentType: 'image/png' });
  await menu.click();
  await page.keyboard.press('Escape');
  await expect(menu).toBeFocused();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(
    true,
  );
  await page.getByRole('button', { name: 'View Markdown' }).click();
  const source = page.getByRole('dialog', { name: 'Repository source' });
  await expect(source.locator('code')).toContainText('# Getting Started');
  await expect(source.locator('code')).toContainText('# Getting Started\n\nCommands below run');
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(
    true,
  );
  await page.keyboard.press('Escape');
  await expect(source).not.toBeVisible();
  await page.setViewportSize({ width: 320, height: 700 });
  await expect
    .poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth))
    .toBe(true);
  expect(problems.errors).toEqual([]);
  expect(await problems.cspViolations()).toEqual([]);
});
