// Copies the static half of @cesium/engine (workers, assets, third-party WASM)
// into public/cesium so Vite serves it in dev and emits it on build. Cesium
// resolves these at runtime from window.CESIUM_BASE_URL.
import { cpSync, existsSync, mkdirSync, rmSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';

const require = createRequire(import.meta.url);
const engineRoot = dirname(require.resolve('@cesium/engine/package.json'));
const target = join(process.cwd(), 'public', 'cesium');
rmSync(target, { recursive: true, force: true });
mkdirSync(target, { recursive: true });
// Workers and ThirdParty (WASM decoders) are built; Assets ship as source.
const sources = { Workers: 'Build/Workers', ThirdParty: 'Build/ThirdParty', Assets: 'Source/Assets' };
for (const [name, rel] of Object.entries(sources)) {
  const from = join(engineRoot, rel);
  if (!existsSync(from)) throw new Error(`@cesium/engine is missing ${rel}`);
  cpSync(from, join(target, name), { recursive: true });
}
console.log(`copied Cesium static assets to ${target}`);
