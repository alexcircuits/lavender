import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from '@playwright/test';

/**
 * Browser performance harness (ENG-1000), kept apart from the e2e suite: its own directory,
 * port and config, so `pnpm run e2e` never runs it. It drives the production build
 * (`apps/web/dist`) served by the RELEASE `lavender-server` in fixture mode, replaying the
 * dense synthetic fixtures that `perf/fixtures.ts` writes before the server starts.
 *
 * Prerequisites (`scripts/perf.sh browser` does all three): `pnpm run build`,
 * `cargo build --release -p lavender-server`, and Chrome (`PLAYWRIGHT_CHANNEL=chrome`).
 * Headless by default: a locked macOS screen stops headed rendering, headless Chrome is
 * unaffected. `LAVENDER_PERF_HEADED=1` shows the window.
 */
export const PERF_PORT = 7428;
const WEB = fileURLToPath(new URL('..', import.meta.url));
const ROOT = resolve(WEB, '../..');
const SCRATCH = join(tmpdir(), 'lavender-perf');
export const FIXTURES = process.env.LAVENDER_PERF_FIXTURES ?? join(SCRATCH, 'fixtures');
export const RESULTS = process.env.LAVENDER_PERF_OUT ?? join(SCRATCH, 'results');
/** One id per run: set when the runner loads this file, inherited by its workers. */
process.env.LAVENDER_PERF_RUN ??= new Date().toISOString().replace(/[:.]/g, '-');
export const RESULTS_FILE = join(RESULTS, `browser-${process.env.LAVENDER_PERF_RUN}.json`);
const SERVER = join(
  process.env.CARGO_TARGET_DIR ?? join(ROOT, 'target'),
  'release',
  'lavender-server',
);
const channel = process.env.PLAYWRIGHT_CHANNEL;

/** The server's whole configuration: fixture mode, this port, scratch directories. */
const SERVER_ENV: Record<string, string> = {
  LAVENDER__SERVER__ADDR: `127.0.0.1:${PERF_PORT}`,
  LAVENDER__SERVER__FIXTURES_DIR: FIXTURES,
  LAVENDER__SERVER__STATIC_DIR: join(WEB, 'dist'),
  LAVENDER__SERVER__DATA_DIR: join(SCRATCH, 'data'),
  LAVENDER__SERVER__CACHE_DIR: join(SCRATCH, 'cache'),
  // The catalog offers a keyed source only when its key is set, also in fixture mode. Fixture
  // mode replaces every provider with a file reader, so this placeholder is never sent anywhere.
  LAVENDER_FIRMS_MAP_KEY: 'perf-fixture-placeholder',
  RUST_LOG: 'warn',
};
/**
 * Inherited variables that would change the measured server (configuration overrides and
 * file, provider keys) are removed, so every run measures the configuration above.
 */
const UNSET = Object.keys(process.env)
  .filter(
    (name) => /^LAVENDER\w*$/.test(name) || ['LL2_API_TOKEN', 'AISSTREAM_API_KEY'].includes(name),
  )
  .filter((name) => !(name in SERVER_ENV))
  .map((name) => `-u ${name}`)
  .join(' ');

export default defineConfig({
  testDir: '.',
  testMatch: 'browser.perf.ts',
  fullyParallel: false,
  workers: 1,
  retries: 0,
  // Five scenarios of bounded waits; the aircraft one waits for two 30 s fixture refreshes.
  timeout: 5 * 60_000,
  reporter: 'list',
  outputDir: join(SCRATCH, 'test-results'),
  use: {
    baseURL: `http://127.0.0.1:${PERF_PORT}`,
    headless: process.env.LAVENDER_PERF_HEADED !== '1',
    ...(channel ? { channel } : {}),
    trace: 'off',
    video: 'off',
    screenshot: 'off',
  },
  webServer: {
    // `--experimental-strip-types` runs the TypeScript generator on every Node the package
    // allows (>= 22.12; type stripping is unflagged only from 22.18).
    command: `node --experimental-strip-types perf/fixtures.ts --out "${FIXTURES}" && exec env ${UNSET} "${SERVER}"`,
    cwd: WEB,
    url: `http://127.0.0.1:${PERF_PORT}/healthz`,
    // Never measure someone else's server on this port.
    reuseExistingServer: false,
    timeout: 60_000,
    env: SERVER_ENV,
  },
});
