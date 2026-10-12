import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig, devices } from '@playwright/test';

/**
 * End-to-end tests against the production build served by the real `lavender-server` in
 * fixture mode (ENG-1003): the Rust parsers, feed hub, HTTP API, WebSocket stream, security
 * headers and CSP are all real, but providers replay the recorded documents in
 * `e2e/fixtures/` instead of contacting live sources. Each snapshot says so.
 *
 * Browsers: `pnpm exec playwright install chromium` once per machine (CI caches it).
 * Without the download, use an installed Chrome: `PLAYWRIGHT_CHANNEL=chrome pnpm run e2e`.
 * The Rust toolchain from `rust-toolchain.toml` must be available (`cargo`).
 */
const PORT = 7412;
const channel = process.env.PLAYWRIGHT_CHANNEL;
const WEB = fileURLToPath(new URL('.', import.meta.url));
const SCRATCH = join(tmpdir(), 'lavender-e2e');

export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [['github'], ['html', { open: 'never' }]] : 'list',
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    trace: 'retain-on-failure',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'], ...(channel ? { channel } : {}) },
    },
  ],
  webServer: {
    command:
      'pnpm run build && cargo run --quiet --manifest-path ../../Cargo.toml -p lavender-server',
    url: `http://127.0.0.1:${PORT}/healthz`,
    reuseExistingServer: !process.env.CI,
    timeout: 300_000,
    env: {
      LAVENDER__SERVER__ADDR: `127.0.0.1:${PORT}`,
      LAVENDER__SERVER__FIXTURES_DIR: join(WEB, 'e2e/fixtures'),
      LAVENDER__SERVER__STATIC_DIR: join(WEB, 'dist'),
      LAVENDER__SERVER__DATA_DIR: join(SCRATCH, 'data'),
      LAVENDER__SERVER__CACHE_DIR: join(SCRATCH, 'cache'),
      RUST_LOG: 'warn',
    },
  },
});
