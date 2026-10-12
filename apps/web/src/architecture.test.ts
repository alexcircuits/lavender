// @vitest-environment node
/// <reference types="node" />
/**
 * The enforceable module boundary (docs/architecture/frontend.md §2, ADR 0004/0005). Biome's
 * noRestrictedImports/noRestrictedGlobals catch most of this at lint time; this test is the
 * backstop that also sees string references, `window.fetch(` and CSS modules.
 */
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const SELF = fileURLToPath(import.meta.url);
const SRC = dirname(SELF);

function walk(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? walk(path) : [path];
  });
}

/** Paths relative to src/, with forward slashes. */
const files = walk(SRC).map((path) => ({
  path,
  rel: relative(SRC, path).split(sep).join('/'),
}));
const sources = files.filter(({ path, rel }) => /\.(ts|tsx)$/.test(rel) && path !== SELF);
const cssModules = files.filter(({ rel }) => rel.endsWith('.module.css'));

function offenders(
  candidates: ReadonlyArray<{ path: string; rel: string }>,
  allowedPrefix: string | null,
  pattern: RegExp,
): string[] {
  return candidates
    .filter(({ rel }) => allowedPrefix === null || !rel.startsWith(allowedPrefix))
    .filter(({ path }) => pattern.test(readFileSync(path, 'utf8')))
    .map(({ rel }) => rel);
}

describe('module boundaries', () => {
  it('scans a non-trivial source tree', () => {
    expect(sources.length).toBeGreaterThan(20);
  });

  it('imports Cesium (@cesium/engine, @cesium/core) only under src/globe', () => {
    expect(offenders(sources, 'globe/', /@cesium\/(engine|core)/)).toEqual([]);
  });

  it('imports @tauri-apps only under src/platform', () => {
    expect(offenders(sources, 'platform/', /@tauri-apps/)).toEqual([]);
  });

  it('calls fetch only under src/platform', () => {
    expect(offenders(sources, 'platform/', /\bfetch\s*\(/)).toEqual([]);
  });

  it('opens WebSockets and EventSources only under src/platform', () => {
    expect(offenders(sources, 'platform/', /\bnew\s+(WebSocket|EventSource)\s*\(/)).toEqual([]);
  });

  it('keeps the design system free of app modules', () => {
    const appImport = /from\s+['"](\.\.\/)+(app|globe|platform|features|state|shell|contracts)\b/;
    expect(
      offenders(
        sources.filter(({ rel }) => rel.startsWith('design/')),
        null,
        appImport,
      ),
    ).toEqual([]);
  });

  it('never statically imports the globe engine module from outside src/globe', () => {
    const staticEngineImport = /^\s*import\s[^;]*['"][./]*globe\/GlobeEngine['"]/m;
    expect(offenders(sources, 'globe/', staticEngineImport)).toEqual([]);
  });
});

describe('CSS modules use tokens', () => {
  const stripComments = (css: string) => css.replace(/\/\*[\s\S]*?\*\//g, '');

  it('finds the CSS modules', () => {
    expect(cssModules.length).toBeGreaterThan(10);
  });

  it('contain no raw colours (hex, rgb, hsl, oklch)', () => {
    const raw = /#[0-9a-f]{3,8}\b|\b(rgba?|hsla?|oklch|oklab|lab|lch)\(/i;
    const bad = cssModules
      .filter(({ path }) => raw.test(stripComments(readFileSync(path, 'utf8'))))
      .map(({ rel }) => rel);
    expect(bad).toEqual([]);
  });

  it('contain no raw z-index values (use the --z-* ladder)', () => {
    const rawZ = /z-index\s*:\s*-?\d/;
    const bad = cssModules
      .filter(({ path }) => rawZ.test(stripComments(readFileSync(path, 'utf8'))))
      .map(({ rel }) => rel);
    expect(bad).toEqual([]);
  });

  it('never squircle globally: corner-shape lives only in tokens.css under .sq', () => {
    const bad = files
      .filter(({ rel }) => rel.endsWith('.css') && rel !== 'design/tokens.css')
      .filter(({ path }) => /corner-shape/.test(stripComments(readFileSync(path, 'utf8'))))
      .map(({ rel }) => rel);
    expect(bad).toEqual([]);
  });
});
