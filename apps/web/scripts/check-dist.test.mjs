import assert from 'node:assert/strict';
import { randomBytes } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { test } from 'node:test';
import { checkDist } from './check-dist.mjs';

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
    }),
  );
  assert.deepEqual(f.check().failures, []);
});
test('rejects unsafe-eval in desktop CSP while accepting wasm-unsafe-eval', (t) =>
  rejects(t, (f) => f.setCsp({ ...f.csp, 'script-src': "'self' 'unsafe-eval'" }), /unsafe-eval/));
test('rejects inline script permission in desktop CSP', (t) =>
  rejects(t, (f) => f.setCsp({ ...f.csp, 'script-src': "'self' 'unsafe-inline'" }), /script-src/));
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
