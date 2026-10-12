// Copies the static half of @cesium/engine into public/cesium so Vite serves it in dev and
// emits it on build. Cesium resolves these at runtime from window.CESIUM_BASE_URL.
//
// @cesium/engine 26.x ships Workers and ThirdParty (WASM decoders) only under Build/, and
// Assets (imagery, IAU2006 tables, approximateTerrainHeights.json) only under Source/. A
// missing directory must fail the build: under tauri:// a missing asset does not 404, the asset
// handler answers with index.html, and Cesium's render loop dies on the parse error
// (docs/architecture/spikes/2026-10-11-tauri-cesium-macos.md, finding 1).
import { cpSync, existsSync, mkdirSync, readdirSync, rmSync, statSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/** Published directory name → path inside the @cesium/engine package. */
const SOURCES = Object.freeze({
  Workers: 'Build/Workers',
  ThirdParty: 'Build/ThirdParty',
  Assets: 'Source/Assets',
});

/** Files whose absence means the copy is unusable even though the directory exists. */
const REQUIRED_FILES = Object.freeze([
  'Assets/approximateTerrainHeights.json',
  'Assets/Textures/NaturalEarthII/tilemapresource.xml',
]);

function resolveEngineRoot() {
  try {
    return dirname(require.resolve('@cesium/engine/package.json', { paths: [appRoot] }));
  } catch (error) {
    throw new Error(`@cesium/engine is not installed (run pnpm install): ${String(error)}`);
  }
}

function assertNonEmptyDirectory(path, label) {
  if (!existsSync(path) || !statSync(path).isDirectory()) {
    throw new Error(`@cesium/engine is missing ${label} (looked in ${path})`);
  }
  if (readdirSync(path).length === 0) {
    throw new Error(`@cesium/engine ${label} is empty (${path})`);
  }
}

function main() {
  const engineRoot = resolveEngineRoot();
  const target = join(appRoot, 'public', 'cesium');

  // Validate everything before touching the output so a failure leaves no half copy.
  for (const rel of Object.values(SOURCES)) assertNonEmptyDirectory(join(engineRoot, rel), rel);

  rmSync(target, { recursive: true, force: true });
  mkdirSync(target, { recursive: true });
  for (const [name, rel] of Object.entries(SOURCES)) {
    cpSync(join(engineRoot, rel), join(target, name), { recursive: true });
  }
  for (const rel of REQUIRED_FILES) {
    if (!existsSync(join(target, rel))) throw new Error(`Cesium asset copy is missing ${rel}`);
  }
  console.log(`Copied Cesium static assets to ${target}`);
}

main();
