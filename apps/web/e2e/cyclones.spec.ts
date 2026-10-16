import { expect, test } from '@playwright/test';
import { litRatio, watchPage } from './support';

test('inspect recorded cyclone positions and forecast geometry through the real Rust feed', async ({
  page,
}, testInfo) => {
  const problems = await watchPage(page);
  await page.goto('/');
  await expect(page.getByText('Loading globe…')).toBeHidden({ timeout: 30_000 });
  const earthquakes = page.getByRole('switch', { name: 'Earthquakes', exact: true });
  if (await earthquakes.isChecked()) await earthquakes.click();
  await page.getByRole('switch', { name: 'Tropical cyclones', exact: true }).click();
  const search = page.getByRole('combobox', { name: 'Search places, coordinates and callsigns' });
  await search.click();
  await search.fill('TEST ALPHA');
  const match = page.getByRole('option', { name: /^TEST ALPHA / });
  await expect(match).toBeVisible({ timeout: 30_000 });
  await expect(match).toHaveAttribute('aria-selected', 'true');
  await search.press('Enter');
  const inspector = page.getByRole('region', { name: 'TEST ALPHA', exact: true });
  await expect(inspector).toBeVisible();
  await expect(inspector.getByText('Maximum sustained wind', { exact: true })).toBeVisible();
  await expect(inspector.getByText(/Current · advisory/)).toBeVisible();
  const forecast = inspector.getByRole('table', { name: /Advisory .* forecast positions/ });
  await expect(forecast).toBeVisible();
  await expect(forecast.getByRole('rowheader', { name: '24 h' })).toBeVisible();
  await inspector.getByRole('button', { name: 'Fly to', exact: true }).click();
  // This status is published by the globe's settled-view event, after the 1.2 s flight.
  await expect(page.getByText('Camera at 400 km', { exact: true })).toBeVisible({
    timeout: 10_000,
  });
  await expect
    .poll(() => litRatio(page, '.lv-globe-host canvas'), { timeout: 30_000 })
    .toBeGreaterThan(0.05);
  await expect
    .poll(
      async () => {
        const image = await page.locator('.lv-globe-host canvas').screenshot();
        return page.evaluate(async (base64) => {
          const png = new Image();
          png.src = `data:image/png;base64,${base64}`;
          await png.decode();
          const canvas = document.createElement('canvas');
          canvas.width = png.naturalWidth;
          canvas.height = png.naturalHeight;
          const context = canvas.getContext('2d');
          if (!context) return 0;
          context.drawImage(png, 0, 0);
          const { data } = context.getImageData(0, 0, canvas.width, canvas.height);
          let orange = 0;
          for (let i = 0; i < data.length; i += 4) {
            const red = data[i] ?? 0;
            if (red > 100 && red > (data[i + 1] ?? 0) * 1.8 && red > (data[i + 2] ?? 0) * 1.8)
              orange += 1;
          }
          return orange;
        }, image.toString('base64'));
      },
      { timeout: 10_000 },
    )
    .toBeGreaterThan(400); // More than the single storm point: track/cone outlines.
  await inspector.getByText('Maximum sustained wind', { exact: true }).scrollIntoViewIfNeeded();
  expect(problems.errors).toEqual([]);
  expect(await problems.cspViolations()).toEqual([]);
  await page.screenshot({ path: testInfo.outputPath('cyclone-forecast.png'), fullPage: true });
  await page.getByRole('switch', { name: 'Tropical cyclones', exact: true }).click();
  await expect(inspector).toBeHidden();
});
