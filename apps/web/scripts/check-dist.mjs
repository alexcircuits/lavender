// Post-build assertions on dist/ (runs as `postbuild`). See security.md §4 and frontend.md §7.
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, extname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
/** docs/performance/specification.md §2: "Initial JS excluding Cesium engine ≤ 250 KiB gzip". */
export const INITIAL_JS_BUDGET_BYTES = 250 * 1024;
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
// Recognisable credential values, not API names in the documentation. Explicit public
// tile tokens belong in the runtime catalog (security.md §3), never in build artifacts.
const KEY_SHAPES = [
  /AIza[0-9A-Za-z_-]{35}/,
  /\bsk-(?:(?:proj|svcacct)-)?[0-9A-Za-z_-]{48,}/,
  /-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----/,
];
const SERVER_KEY_ASSIGNMENT =
  /\b(?:LAVENDER_)?(?:FIRMS_MAP_KEY|AISSTREAM_API_KEY|OPENSKY_CLIENT_ID|OPENSKY_CLIENT_SECRET|LL2_API_TOKEN|TOMTOM_API_KEY|GOOGLE_MAPS_SERVER_KEY|OPENAI_API_KEY)\b["']?\s*[:=]\s*["']([0-9A-Za-z_-]{16,})["']/g;

function hasJwt(source) {
  // A complete signed JWT has three base64url parts. Decode the JSON rather than
  // flagging prose, placeholder examples or JavaScript member-expression names.
  const shape = /\b(eyJ[0-9A-Za-z_-]{5,1021})\.([0-9A-Za-z_-]{10,65536})\.[0-9A-Za-z_-]{16,}/g;
  return [...source.matchAll(shape)].some(([, head, claims]) => {
    try {
      const header = JSON.parse(Buffer.from(head, 'base64url').toString('utf8'));
      const payload = JSON.parse(Buffer.from(claims, 'base64url').toString('utf8'));
      return (
        typeof header?.alg === 'string' &&
        payload !== null &&
        typeof payload === 'object' &&
        !Array.isArray(payload)
      );
    } catch {
      return false;
    }
  });
}

function walk(dir) {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? walk(path) : [path];
  });
}

/** Attribute values may themselves contain prose such as "style="; only real attributes count. */
function attributes(source) {
  return new Map(
    [...source.matchAll(/([^\s=/'"<>]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+)))?/g)].map(
      ([, name, quoted, single, bare]) => [name.toLowerCase(), quoted ?? single ?? bare ?? ''],
    ),
  );
}

function checkIndexHtml(source, label, fail) {
  const html = source.replace(/<!--[\s\S]*?-->/g, '');
  // A quoted > is part of an attribute, not the end of a tag. Text/code samples use
  // escaped markup, so they do not become executable tags during this scan.
  for (const match of html.matchAll(/<([a-z][\w:-]*)\b((?:"[^"]*"|'[^']*'|[^'"<>])*)>/gi)) {
    const [tag, name, raw] = match;
    const attrs = attributes(raw);
    if (name.toLowerCase() === 'script') {
      const body = html.slice(match.index + tag.length).split(/<\/script\s*>/i)[0];
      if (!attrs.has('src') || body.trim() !== '') fail(`${label} contains an inline <script>`);
    }
    if (name.toLowerCase() === 'style') fail(`${label} contains an inline <style>`);
    if (attrs.has('style')) fail(`${label} contains a style= attribute`);
    if ([...attrs.keys()].some((key) => /^on[a-z]+$/.test(key))) {
      fail(`${label} contains an inline event handler`);
    }
  }
}

function initialScripts(html, dist) {
  const paths = [];
  for (const [, name, raw] of html.matchAll(/<(script|link)\b((?:"[^"]*"|'[^']*'|[^'"<>])*)>/gi)) {
    const attrs = attributes(raw);
    const href =
      name.toLowerCase() === 'script'
        ? attrs.get('src')
        : attrs.get('rel') === 'modulepreload'
          ? attrs.get('href')
          : undefined;
    if (href) paths.push(join(dist, href.replace(/^\//, '')));
  }
  return [...new Set(paths)];
}

function checkSecrets(files, root, fail) {
  for (const file of files) {
    const buffer = readFileSync(file);
    const source = buffer.toString(
      TEXT_EXTENSIONS.has(extname(file).toLowerCase()) ? 'utf8' : 'latin1',
    );
    const namedKey = [...source.matchAll(SERVER_KEY_ASSIGNMENT)].some(
      ([, value]) => !/^(YOUR_|EXAMPLE_|REPLACE_|PLACEHOLDER)/i.test(value),
    );
    if (KEY_SHAPES.some((shape) => shape.test(source)) || namedKey || hasJwt(source)) {
      // Never include the matched credential in diagnostic output.
      fail(`${relative(root, file)} contains a credential value`);
    }
  }
}

function checkEngineSplit(html, files, dist, root, label, fail) {
  const initial = initialScripts(html, dist);
  if (initial.length === 0) fail(`${label} loads no module script`);
  let initialGzip = 0;
  for (const file of initial) {
    if (!existsSync(file)) {
      fail(`${label} references a missing file: ${relative(dist, file)}`);
      continue;
    }
    const code = readFileSync(file);
    initialGzip += gzipSync(code).length;
    if (code.toString('utf8').includes(ENGINE_MARKER)) {
      fail(`${relative(root, file)} is loaded at startup but contains Cesium engine code`);
    }
  }
  const lazyEngine = files.filter(
    (file) =>
      extname(file) === '.js' &&
      !initial.includes(file) &&
      !file.startsWith(`${join(dist, 'cesium')}${sep}`) &&
      readFileSync(file, 'utf8').includes(ENGINE_MARKER),
  );
  if (lazyEngine.length === 0) fail('No lazily loaded chunk contains the Cesium engine');
  if (initialGzip > INITIAL_JS_BUDGET_BYTES) {
    fail(
      `${label} initial JS is ${(initialGzip / 1024).toFixed(1)} KiB gzip; the budget is ${INITIAL_JS_BUDGET_BYTES / 1024} KiB`,
    );
  }
  return { initialGzip, initialCount: initial.length, lazyEngine };
}

function checkDesktopCsp(path, fail) {
  if (!existsSync(path)) {
    fail('Desktop CSP configuration is missing');
    return;
  }
  let csp;
  try {
    csp = JSON.parse(readFileSync(path, 'utf8')).app?.security?.csp;
  } catch {
    fail('Desktop CSP configuration is not valid JSON');
    return;
  }
  if (!csp || typeof csp !== 'object' || Array.isArray(csp)) {
    fail('Desktop CSP must be an explicit directive map');
    return;
  }
  const directives = new Map();
  for (const [name, value] of Object.entries(csp)) {
    // Tauri serializes this map verbatim. Invalid names and raw separators can
    // inject a new directive that the structured checks below would never see.
    if (!/^[A-Za-z][A-Za-z0-9-]*$/.test(name)) {
      fail('Desktop CSP contains an invalid directive name');
      continue;
    }
    const values = Array.isArray(value) ? value : [value];
    if (values.some((raw) => typeof raw === 'string' && /[;\p{Cc}\p{Zl}\p{Zp}]/u.test(raw))) {
      fail('Desktop CSP contains a directive separator or control character');
    }
    const directive = name.toLowerCase();
    if (directives.has(directive)) fail('Desktop CSP contains a duplicate directive');
    directives.set(directive, value);
    if (typeof value !== 'string' || value.split(/\s+/).includes("'unsafe-eval'")) {
      fail('Desktop CSP contains unsafe-eval or an invalid directive');
    }
  }
  const tokens = (directive) => {
    const value = directives.get(directive);
    return typeof value === 'string' ? value.trim().split(/\s+/) : [];
  };
  const script = tokens('script-src');
  if (
    !script.includes("'self'") ||
    script.some((value) => !["'self'", "'wasm-unsafe-eval'"].includes(value))
  ) {
    fail('Desktop script-src must allow only self and the explicit WASM exception');
  }
  // These CSP3 directives override script-src for their own execution contexts.
  // An otherwise strict base policy cannot make a permissive override safe.
  if (directives.has('script-src-elem')) {
    const elements = tokens('script-src-elem');
    if (elements.length !== 1 || !["'self'", "'none'"].includes(elements[0])) {
      fail('Desktop script-src-elem must allow only self or disable script elements');
    }
  }
  if (directives.has('script-src-attr')) {
    const attributes = tokens('script-src-attr');
    if (attributes.length !== 1 || attributes[0] !== "'none'") {
      fail('Desktop script-src-attr must disable script attributes');
    }
  }
}

/** Pure result for a supplied build tree; negative fixtures exercise the same checks as postbuild. */
export function checkDist({
  dist = join(appRoot, 'dist'),
  root = appRoot,
  desktopConfig = resolve(appRoot, '../desktop/src-tauri/tauri.conf.json'),
} = {}) {
  const failures = [];
  const fail = (message) => failures.push(message);
  const indexPath = join(dist, 'index.html');
  if (!existsSync(indexPath))
    return { failures: ['dist/index.html does not exist; run the build first'] };
  const html = readFileSync(indexPath, 'utf8');
  const files = walk(dist);
  checkIndexHtml(html, 'dist/index.html', fail);
  const docsPath = join(dist, 'docs/index.html');
  let docsSplit;
  if (!existsSync(docsPath)) fail('dist/docs/index.html is missing');
  else {
    const docsHtml = readFileSync(docsPath, 'utf8');
    checkIndexHtml(docsHtml, 'dist/docs/index.html', fail);
    docsSplit = checkEngineSplit(docsHtml, files, dist, root, 'Documentation', fail);
  }
  checkSecrets(files, root, fail);
  checkDesktopCsp(desktopConfig, fail);
  const split = checkEngineSplit(html, files, dist, root, 'Workspace', fail);
  for (const dir of ['Workers', 'ThirdParty', 'Assets']) {
    const path = join(dist, 'cesium', dir);
    if (!existsSync(path) || readdirSync(path).length === 0) fail(`dist/cesium/${dir} is missing`);
  }
  return { failures, files, split, docsSplit };
}

/** The postbuild command: prints the outcome and returns the process exit code (1 fails CI). */
function run(options = {}) {
  const result = checkDist(options);
  if (result.failures.length > 0) {
    console.error(`check-dist: ${result.failures.length} problem(s)`);
    for (const message of result.failures) console.error(`  ✗ ${message}`);
    return 1;
  }
  const dist = options.dist ?? join(appRoot, 'dist');
  const engine = result.split.lazyEngine.map((f) => relative(dist, f)).join(', ');
  console.log(
    `check-dist: ok — ${result.files.length} files; workspace JS ${(result.split.initialGzip / 1024).toFixed(1)} KiB gzip; docs JS ${(result.docsSplit.initialGzip / 1024).toFixed(1)} KiB gzip; engine chunk ${engine}`,
  );
  return 0;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exitCode = run();
}
