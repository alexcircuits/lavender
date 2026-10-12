// Post-build assertions on dist/ (runs as `postbuild`). Each check maps to a written rule:
//
// 1. index.html has no inline <script>, <style> or style= (CSP; Tauri hashes inline blocks,
//    which would void style-src 'unsafe-inline' — docs/architecture/tauri.md §5.2).
// 2. Nothing in dist/ carries a Google API key ("AIza…"): browser builds never embed keys
//    (docs/architecture/security.md, ADR 0006).
// 3. @cesium/engine is a separate lazily loaded chunk: the initial JS (entry + modulepreloads)
//    contains no engine code, and some other chunk does (frontend.md §7).
// 4. Initial JS (excluding the engine) is ≤ 250 KiB gzip (frontend.md §7 budget).
// 5. Cesium's static directories were emitted (spike finding 1).
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, extname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const dist = join(appRoot, 'dist');

const INITIAL_JS_BUDGET_BYTES = 250 * 1024;
/** Present in every Cesium build (GLSL built-ins are prefixed czm_) and nowhere else. */
const ENGINE_MARKER = 'czm_';
const TEXT_EXTENSIONS = new Set([
  '.html',
  '.js',
  '.mjs',
  '.css',
  '.json',
  '.map',
  '.txt',
  '.xml',
  '.svg',
  '.glsl',
]);
/** The full Google API key shape, used for binary files where 4 bytes can occur by chance. */
const GOOGLE_KEY_PATTERN = /AIza[0-9A-Za-z_-]{35}/;

const failures = [];
const fail = (message) => failures.push(message);

function walk(dir) {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? walk(path) : [path];
  });
}

function checkIndexHtml(source) {
  // Comments never execute; they are stripped so prose about the rule cannot trip it.
  const html = source.replace(/<!--[\s\S]*?-->/g, '');
  const scripts = [...html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/gi)];
  for (const [, attrs, body] of scripts) {
    if (!/\bsrc\s*=/.test(attrs ?? '') || (body ?? '').trim() !== '') {
      fail('dist/index.html contains an inline <script>');
    }
  }
  if (/<style\b/i.test(html)) fail('dist/index.html contains an inline <style>');
  if (/\sstyle\s*=/i.test(html)) fail('dist/index.html contains a style= attribute');
  if (/\son[a-z]+\s*=/i.test(html)) fail('dist/index.html contains an inline event handler');
}

function initialScripts(html) {
  const entry = [...html.matchAll(/<script\b[^>]*\bsrc="([^"]+)"/gi)].map((m) => m[1]);
  const preloads = [...html.matchAll(/<link\b[^>]*rel="modulepreload"[^>]*href="([^"]+)"/gi)].map(
    (m) => m[1],
  );
  return [...entry, ...preloads].map((href) => join(dist, href.replace(/^\//, '')));
}

function checkSecrets(files) {
  for (const file of files) {
    const buffer = readFileSync(file);
    const isText = TEXT_EXTENSIONS.has(extname(file).toLowerCase());
    const leaked = isText
      ? buffer.toString('utf8').includes('AIza')
      : GOOGLE_KEY_PATTERN.test(buffer.toString('latin1'));
    if (leaked) fail(`${relative(appRoot, file)} contains "AIza" (a Google API key shape)`);
  }
}

function checkEngineSplit(html, files) {
  const initial = initialScripts(html);
  if (initial.length === 0) fail('dist/index.html loads no module script');
  let initialGzip = 0;
  for (const file of initial) {
    if (!existsSync(file)) {
      fail(`dist/index.html references a missing file: ${relative(dist, file)}`);
      continue;
    }
    const code = readFileSync(file);
    initialGzip += gzipSync(code).length;
    if (code.toString('utf8').includes(ENGINE_MARKER)) {
      fail(`${relative(appRoot, file)} is loaded at startup but contains Cesium engine code`);
    }
  }
  const lazyEngine = files.filter(
    (file) =>
      extname(file) === '.js' &&
      !initial.includes(file) &&
      !file.includes(`${join(dist, 'cesium')}`) &&
      readFileSync(file, 'utf8').includes(ENGINE_MARKER),
  );
  if (lazyEngine.length === 0) fail('No lazily loaded chunk contains the Cesium engine');
  if (initialGzip > INITIAL_JS_BUDGET_BYTES) {
    fail(`Initial JS is ${(initialGzip / 1024).toFixed(1)} KiB gzip; the budget is 250 KiB`);
  }
  return { initialGzip, initialCount: initial.length, lazyEngine };
}

function checkCesiumStatic() {
  for (const dir of ['Workers', 'ThirdParty', 'Assets']) {
    const path = join(dist, 'cesium', dir);
    if (!existsSync(path) || readdirSync(path).length === 0) fail(`dist/cesium/${dir} is missing`);
  }
}

function main() {
  const indexPath = join(dist, 'index.html');
  if (!existsSync(indexPath)) {
    console.error('check-dist: dist/index.html does not exist; run the build first');
    process.exit(1);
  }
  const html = readFileSync(indexPath, 'utf8');
  const files = walk(dist);

  checkIndexHtml(html);
  checkSecrets(files);
  const split = checkEngineSplit(html, files);
  checkCesiumStatic();

  if (failures.length > 0) {
    console.error(`check-dist: ${failures.length} problem(s)`);
    for (const message of failures) console.error(`  ✗ ${message}`);
    process.exit(1);
  }
  const engine = split.lazyEngine.map((f) => relative(dist, f)).join(', ');
  console.log(
    `check-dist: ok — ${files.length} files; initial JS ${(split.initialGzip / 1024).toFixed(1)} KiB gzip in ${split.initialCount} file(s); engine chunk ${engine}`,
  );
}

main();
