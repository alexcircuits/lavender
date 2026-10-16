import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import {
  copyFileSync,
  cpSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';
import { checkDist, INITIAL_JS_BUDGET_BYTES } from './check-dist.mjs';

const HTML = '<!doctype html><script type="module" src="/assets/app.js"></script>';
function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'lavender-dist-security-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const dist = join(root, 'dist');
  const desktopConfig = join(root, 'tauri.conf.json');
  const write = (path, value) => {
    const target = join(dist, path);
    mkdirSync(dirname(target), { recursive: true });
    writeFileSync(target, value);
  };
  write('index.html', HTML);
  write('docs/index.html', HTML);
  write('assets/app.js', 'console.log("fixture entry");');
  write('assets/engine.js', 'const shader = "czm_fixture";');
  for (const dir of ['Workers', 'ThirdParty', 'Assets'])
    write(`cesium/${dir}/fixture.js`, '// fixture');
  const csp = {
    'default-src': "'self'",
    'script-src': "'self' 'wasm-unsafe-eval'",
    'style-src': "'self' 'unsafe-inline'",
  };
  const setCsp = (value) =>
    writeFileSync(desktopConfig, JSON.stringify({ app: { security: { csp: value } } }));
  setCsp(csp);
  return {
    root,
    dist,
    desktopConfig,
    write,
    setCsp,
    csp,
    check: () => checkDist({ root, dist, desktopConfig }),
    html: () => readFileSync(join(dist, 'index.html'), 'utf8'),
  };
}

function rejects(t, mutate, pattern) {
  const f = fixture(t);
  assert.deepEqual(f.check().failures, [], 'the control fixture must first pass');
  mutate(f);
  assert.match(f.check().failures.join('\n'), pattern, 'the planted violation must be detected');
}

test('accepts external scripts, comments, attributes containing prose, and documentation code samples', (t) => {
  const f = fixture(t);
  f.write(
    'index.html',
    `${HTML}<div data-note="style='example' onclick='example' >">&lt;script&gt;example&lt;/script&gt;</div><!-- <style>example</style> -->`,
  );
  f.write(
    'assets/docs.js',
    `const example = "eval(), new Function(), unsafe-eval, innerHTML, AIza, OPENAI_API_KEY, FIRMS_MAP_KEY, JWT header.payload.signature, eyJplaceholder.invalidJSON.signaturePlaceholder";`,
  );
  assert.deepEqual(f.check().failures, []);
});

for (const [name, markup, pattern] of [
  ['inline script', '<script>alert(1)</script>', /inline <script>/],
  [
    'external script with executable body',
    '<script src="/assets/app.js">alert(1)</script>',
    /inline <script>/,
  ],
  [
    'fake src in another attribute',
    '<script data-note="src=external">alert(1)</script>',
    /inline <script>/,
  ],
  ['inline style', '<style>body { color: red }</style>', /inline <style>/],
  ['style attribute', '<div STYLE = "color:red"></div>', /style= attribute/],
  ['unquoted event handler', '<img src=x onerror=alert(1)>', /inline event handler/],
]) {
  for (const page of ['index.html', 'docs/index.html']) {
    test(`rejects ${name} in ${page}`, (t) =>
      rejects(t, (f) => f.write(page, `${HTML}${markup}`), pattern));
  }
}

for (const [name, value] of [
  ['Google', `AI${'za'}${'A'.repeat(35)}`],
  ['OpenAI project', `s${'k'}-proj-${'a'.repeat(60)}`],
  ['OpenAI legacy', `s${'k'}-${'b'.repeat(48)}`],
  ['PEM', ['-----BEGIN RSA PRIVATE', 'KEY-----'].join(' ')],
  ['named server key', JSON.stringify({ AISSTREAM_API_KEY: 'syntheticCredential1234567890' })],
  [
    'JWT',
    [
      Buffer.from(JSON.stringify({ alg: 'HS256', typ: 'JWT' })).toString('base64url'),
      Buffer.from(JSON.stringify({ iss: 'lavender.invalid', sub: 'fixture' })).toString(
        'base64url',
      ),
      Buffer.alloc(32, 1).toString('base64url'),
    ].join('.'),
  ],
]) {
  test(`rejects a synthetic ${name} credential without echoing it`, (t) => {
    const f = fixture(t);
    assert.deepEqual(f.check().failures, []);
    f.write('assets/leak.js', value);
    const diagnostics = f.check().failures.join('\n');
    assert.match(diagnostics, /leak\.js contains a credential value/);
    assert.ok(!diagnostics.includes(value), 'diagnostics must not repeat the credential');
  });
}

test('scans binary assets for complete credential shapes', (t) =>
  rejects(
    t,
    (f) => f.write('assets/leak.bin', Buffer.from(`\0AI${'za'}${'B'.repeat(35)}\0`)),
    /credential value/,
  ));
test('allows server key names and explicit placeholder examples', (t) => {
  const f = fixture(t);
  f.write(
    'assets/example.json',
    JSON.stringify({
      FIRMS_MAP_KEY: 'YOUR_FIRMS_MAP_KEY',
      TOMTOM_API_KEY: 'EXAMPLE_PLACEHOLDER_TOKEN',
      LAVENDER_AISSTREAM_API_KEY: 'YOUR_AISSTREAM_API_KEY',
      LAVENDER_FIRMS_MAP_KEY: 'EXAMPLE_PLACEHOLDER_MAP_KEY',
      LAVENDER_OPENSKY_CLIENT_SECRET: 'REPLACE_OPENSKY_CLIENT_SECRET',
    }),
  );
  assert.deepEqual(f.check().failures, []);
});
for (const name of [
  'LAVENDER_AISSTREAM_API_KEY',
  'LAVENDER_FIRMS_MAP_KEY',
  'LAVENDER_OPENSKY_CLIENT_SECRET',
  'LAVENDER_OPENSKY_CLIENT_ID',
  'OPENSKY_CLIENT_ID',
  'LAVENDER_LL2_API_TOKEN',
  'LL2_API_TOKEN',
]) {
  test(`rejects an opaque credential labelled ${name} without echoing it`, (t) => {
    const f = fixture(t);
    const value = 'syntheticOpaqueCredential1234567890';
    assert.deepEqual(f.check().failures, []);
    f.write('assets/runtime.json', JSON.stringify({ [name]: value }));
    const diagnostics = f.check().failures.join('\n');
    assert.match(diagnostics, /runtime\.json contains a credential value/);
    assert.ok(!diagnostics.includes(value));
  });
}
test('rejects unsafe-eval in desktop CSP while accepting wasm-unsafe-eval', (t) =>
  rejects(t, (f) => f.setCsp({ ...f.csp, 'script-src': "'self' 'unsafe-eval'" }), /unsafe-eval/));
test('rejects inline script permission in desktop CSP', (t) =>
  rejects(t, (f) => f.setCsp({ ...f.csp, 'script-src': "'self' 'unsafe-inline'" }), /script-src/));
for (const directive of ['script-src-elem', 'script-src-attr', 'SCRIPT-SRC-ELEM']) {
  for (const value of ["'unsafe-inline'", 'https://scripts.example.invalid']) {
    test(`rejects ${value} overriding ${directive}`, (t) =>
      rejects(t, (f) => f.setCsp({ ...f.csp, [directive]: value }), /script-src-(elem|attr)/));
  }
}
test('allows explicit self-only script elements and disables script attributes', (t) => {
  const f = fixture(t);
  f.setCsp({ ...f.csp, 'script-src-elem': "'self'", 'script-src-attr': "'none'" });
  assert.deepEqual(f.check().failures, []);
  f.setCsp({ ...f.csp, 'script-src-elem': "'none'", 'script-src-attr': "'none'" });
  assert.deepEqual(f.check().failures, []);
});
test('rejects conflicting CSP directives after case normalization', (t) =>
  rejects(t, (f) => f.setCsp({ ...f.csp, 'SCRIPT-SRC': "'self'" }), /duplicate directive/));
for (const name of [
  'ignored; script-src-elem',
  ' script-src-elem',
  'script-src-elem ',
  'ignored\nscript-src-elem',
  'script-src-élem',
]) {
  test(`rejects a CSP directive name outside the serialization grammar: ${JSON.stringify(name)}`, (t) =>
    rejects(t, (f) => f.setCsp({ ...f.csp, [name]: "'unsafe-inline'" }), /invalid directive name/));
}
test('rejects a second CSP directive injected through an unrelated value', (t) =>
  rejects(
    t,
    (f) => f.setCsp({ ...f.csp, 'connect-src': "'self'; script-src-elem 'unsafe-inline'" }),
    /separator or control/,
  ));
for (const delimiter of ['\n', '\r', '\t', '\0', '\x1b', '\x7f', '\x85', '\u2028', '\u2029']) {
  test(`rejects a CSP value control/line delimiter: ${JSON.stringify(delimiter)}`, (t) =>
    rejects(
      t,
      (f) => f.setCsp({ ...f.csp, 'connect-src': `'self'${delimiter}https://example.invalid` }),
      /separator or control/,
    ));
}
for (const value of [
  "'self'; script-src-elem 'unsafe-inline'",
  "'self'\nhttps://example.invalid",
]) {
  test(`rejects serialization delimiters in a CSP array element: ${JSON.stringify(value)}`, (t) =>
    rejects(t, (f) => f.setCsp({ ...f.csp, 'connect-src': [value] }), /separator or control/));
}
test('rejects a non-string script directive without throwing or echoing it', (t) =>
  rejects(
    t,
    (f) => f.setCsp({ ...f.csp, 'script-src': ['config-secret-sentinel'] }),
    /invalid directive/,
  ));
test('rejects a disabled desktop CSP', (t) =>
  rejects(t, (f) => f.setCsp(null), /explicit directive map/));
test('reports malformed CSP configuration without repeating its contents', (t) => {
  const f = fixture(t);
  writeFileSync(f.desktopConfig, '{ invalid-config-secret-sentinel');
  const diagnostics = f.check().failures.join('\n');
  assert.match(diagnostics, /not valid JSON/);
  assert.ok(!diagnostics.includes('invalid-config-secret-sentinel'));
});
test('rejects a missing entry chunk', (t) =>
  rejects(t, (f) => f.write('index.html', '<script src="/missing.js"></script>'), /missing file/));
test('rejects an eager engine chunk', (t) =>
  rejects(
    t,
    (f) => f.write('index.html', `${HTML}<link rel="modulepreload" href="/assets/engine.js">`),
    /loaded at startup/,
  ));
test('rejects a missing lazy engine', (t) =>
  rejects(t, (f) => f.write('assets/engine.js', '// no engine'), /No lazily loaded chunk/));
test('rejects entry JavaScript exceeding the gzip budget', (t) =>
  rejects(
    t,
    (f) => f.write('assets/app.js', randomBytes(512 * 1024).toString('hex')),
    /budget is 250 KiB/,
  ));
test('rejects missing Cesium static assets', (t) =>
  rejects(
    t,
    (f) => rmSync(join(f.dist, 'cesium/Workers'), { recursive: true }),
    /cesium\/Workers is missing/,
  ));

/** Deterministic, incompressible bytes (xorshift32), so gzip sizes are predictable. */
function noise(size, seed = 0x2545f491) {
  const bytes = Buffer.alloc(size);
  let x = seed;
  for (let i = 0; i < size; i += 1) {
    x ^= x << 13;
    x ^= x >>> 17;
    x ^= x << 5;
    bytes[i] = x & 0xff;
  }
  return bytes;
}

test('the initial-JS budget is the one in docs/performance/specification.md', () => {
  const spec = readFileSync(
    new URL('../../../docs/performance/specification.md', import.meta.url),
    'utf8',
  );
  const row = spec.match(/\|\s*Initial JS excluding Cesium engine\s*\|\s*≤\s*(\d+)\s*KiB gzip/);
  assert.ok(row, 'the specification states the initial-JS budget');
  assert.equal(INITIAL_JS_BUDGET_BYTES, Number(row[1]) * 1024);
});

test('the budget holds at its edge: just under passes, just over fails', (t) => {
  const f = fixture(t);
  const under = noise(INITIAL_JS_BUDGET_BYTES - 2048);
  assert.ok(gzipSync(under).length <= INITIAL_JS_BUDGET_BYTES);
  f.write('assets/app.js', under);
  assert.deepEqual(f.check().failures, []);
  const over = noise(INITIAL_JS_BUDGET_BYTES + 1024);
  assert.ok(gzipSync(over).length > INITIAL_JS_BUDGET_BYTES);
  f.write('assets/app.js', over);
  assert.match(f.check().failures.join('\n'), /Workspace initial JS is 25\d\.\d KiB gzip/);
});

test('modulepreloaded chunks count towards the budget', (t) =>
  rejects(
    t,
    (f) => {
      const half = noise(INITIAL_JS_BUDGET_BYTES / 2 + 4096);
      f.write('assets/app.js', half);
      f.write('assets/vendor.js', noise(INITIAL_JS_BUDGET_BYTES / 2 + 4096, 7));
      f.write('index.html', `${HTML}<link rel="modulepreload" href="/assets/vendor.js">`);
    },
    /Workspace initial JS .* the budget is 250 KiB/,
  ));

test('the documentation page has the same budget', (t) =>
  rejects(
    t,
    (f) => {
      f.write('assets/docs-entry.js', noise(INITIAL_JS_BUDGET_BYTES + 1024));
      f.write('docs/index.html', '<script type="module" src="/assets/docs-entry.js"></script>');
    },
    /Documentation initial JS .* the budget is 250 KiB/,
  ));

/**
 * Runs `node scripts/check-dist.mjs` exactly as the `postbuild` script does, on a copy of the
 * script placed in a repository-shaped tree around the fixture (`web/dist`,
 * `desktop/src-tauri/tauri.conf.json`), so the test sees the real process exit status.
 */
function postbuild(f) {
  const web = join(f.root, 'repo', 'web');
  mkdirSync(join(web, 'scripts'), { recursive: true });
  copyFileSync(
    fileURLToPath(new URL('./check-dist.mjs', import.meta.url)),
    join(web, 'scripts/check-dist.mjs'),
  );
  rmSync(join(web, 'dist'), { recursive: true, force: true });
  cpSync(f.dist, join(web, 'dist'), { recursive: true });
  mkdirSync(join(f.root, 'repo', 'desktop', 'src-tauri'), { recursive: true });
  copyFileSync(f.desktopConfig, join(f.root, 'repo', 'desktop', 'src-tauri', 'tauri.conf.json'));
  return spawnSync(process.execPath, ['scripts/check-dist.mjs'], { cwd: web, encoding: 'utf8' });
}

test('the postbuild process exits 1 when a budget is exceeded, so the build fails', (t) => {
  const f = fixture(t);
  const passing = postbuild(f);
  assert.equal(passing.status, 0, passing.stderr);
  assert.match(passing.stdout, /check-dist: ok/);
  f.write('assets/app.js', noise(INITIAL_JS_BUDGET_BYTES + 1024));
  const failing = postbuild(f);
  assert.equal(failing.status, 1, failing.stdout);
  assert.match(failing.stderr, /Workspace initial JS .* the budget is 250 KiB/);
});
