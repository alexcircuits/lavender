// @vitest-environment node
/// <reference types="node" />
/**
 * Spec ↔ code drift test for docs/design/design-system.md and src/design/tokens.css.
 *
 * It reads the design system itself, so a token added to or changed in the spec fails here
 * until tokens.css follows (and vice versa for the values it checks). It also re-measures the
 * WCAG contrast pairs of §3 and verifies the reduced-motion, more-contrast and forced-colours
 * swaps.
 */
import { readFileSync } from 'node:fs';
import { dirname, resolve as resolvePath } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const HERE = dirname(fileURLToPath(import.meta.url));
const SPEC_PATH = resolvePath(HERE, '../../../../docs/design/design-system.md');
const TOKENS_PATH = resolvePath(HERE, 'tokens.css');

const spec = readFileSync(SPEC_PATH, 'utf8');
const css = readFileSync(TOKENS_PATH, 'utf8').replace(/\/\*[\s\S]*?\*\//g, '');

// ── A tiny CSS block reader (enough for a token sheet) ──────────────────────────────────────

interface Block {
  prelude: string;
  body: string;
}

function topLevelBlocks(source: string): Block[] {
  const blocks: Block[] = [];
  let depth = 0;
  let start = 0;
  let preludeStart = 0;
  for (let i = 0; i < source.length; i += 1) {
    const ch = source[i];
    if (ch === '{') {
      if (depth === 0) {
        start = i;
      }
      depth += 1;
    } else if (ch === '}') {
      depth -= 1;
      if (depth === 0) {
        blocks.push({
          prelude: source.slice(preludeStart, start).trim(),
          body: source.slice(start + 1, i),
        });
        preludeStart = i + 1;
      }
    } else if (ch === ';' && depth === 0) {
      preludeStart = i + 1;
    }
  }
  return blocks;
}

function declarations(body: string): Map<string, string> {
  const map = new Map<string, string>();
  for (const match of body.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) {
    const [, name, value] = match;
    if (name && value) map.set(name, value.replace(/\s+/g, ' ').trim());
  }
  return map;
}

function rootIn(blocks: Block[]): Map<string, string> {
  const root = blocks.find((block) => block.prelude === ':root');
  return root ? declarations(root.body) : new Map();
}

function mediaRoot(query: string): Map<string, string> {
  const media = topLevelBlocks(css).find((block) => block.prelude === `@media ${query}`);
  return media ? rootIn(topLevelBlocks(media.body)) : new Map();
}

const blocks = topLevelBlocks(css);
const base = rootIn(blocks);
const registered = new Set(
  blocks
    .filter((block) => block.prelude.startsWith('@property'))
    .map((block) => block.prelude.replace('@property', '').trim()),
);

function token(name: string): string {
  const value = base.get(name);
  if (value === undefined) throw new Error(`tokens.css does not define ${name}`);
  return value;
}

/** Resolves var() chains against the base :root. */
function resolve(value: string, scope: Map<string, string> = base): string {
  const ref = value.match(/^var\((--[\w-]+)\)$/);
  if (!ref?.[1]) return value;
  const next = scope.get(ref[1]) ?? base.get(ref[1]);
  if (next === undefined) throw new Error(`unresolved ${ref[1]}`);
  return resolve(next, scope);
}

/** Normalises numbers so `.2` and `0.2`, `cubic-bezier(.2,0,0,1)` and `(0.2, 0, 0, 1)` match. */
function normalise(value: string): string {
  return value
    .toLowerCase()
    .replace(/\s+/g, '')
    .replace(/(^|[^\d])\.(\d)/g, '$10.$2');
}

// ── WCAG 2.x contrast ───────────────────────────────────────────────────────────────────────

function luminance(hex: string): number {
  const n = Number.parseInt(hex.replace('#', ''), 16);
  const channel = (c: number) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return (
    0.2126 * channel((n >> 16) & 255) + 0.7152 * channel((n >> 8) & 255) + 0.0722 * channel(n & 255)
  );
}

function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
}

const hexOf = (name: string) => resolve(token(name));

// ── Spec extraction ─────────────────────────────────────────────────────────────────────────

function section(heading: RegExp): string {
  const start = spec.search(heading);
  if (start < 0) throw new Error(`design-system.md has no section matching ${heading}`);
  const rest = spec.slice(start + 1);
  const end = rest.search(/\n## /);
  return end < 0 ? rest : rest.slice(0, end);
}

describe('design-system.md ↔ tokens.css', () => {
  it('defines every custom property the design system names', () => {
    const named = new Set(
      [...spec.matchAll(/(?<![\w-])(--[a-z][a-z0-9_-]*[a-z0-9])/g)].map((m) => m[1] as string),
    );
    expect(named.size).toBeGreaterThanOrEqual(55);
    const missing = [...named].filter((name) => !base.has(name) && !registered.has(name));
    expect(missing).toEqual([]);
  });

  it('matches every colour value in the surface, accent and status tables (§3)', () => {
    const rows = [...section(/^## 3\. Colour/m).matchAll(/\|\s*`(--[\w-]+)`\s*\|\s*`([^`]+)`/g)];
    expect(rows.length).toBeGreaterThanOrEqual(20);
    for (const [, name, value] of rows) {
      expect(normalise(token(name as string)), name).toBe(
        normalise((value as string).startsWith('--') ? `var(${value})` : (value as string)),
      );
    }
  });

  it('defines every map family colour and the hazard ramp (§4)', () => {
    const hexes = new Set(
      [...section(/^## 4\. Map symbology/m).matchAll(/#[0-9a-f]{6}\b/gi)].map((m) =>
        m[0].toLowerCase(),
      ),
    );
    expect(hexes.size).toBeGreaterThanOrEqual(10);
    const values = new Set([...base.values()].map((v) => v.toLowerCase()));
    expect([...hexes].filter((hex) => !values.has(hex))).toEqual([]);
    expect(['1', '2', '3', '4', '5'].map((n) => hexOf(`--hazard-${n}`))).toEqual([
      '#8f3c19',
      '#b9471d',
      '#d95926',
      '#ef7a45',
      '#f8a57a',
    ]);
  });

  it('matches the type scale (§2)', () => {
    const rows = [
      ...section(/^## 2\. Typography/m).matchAll(
        /\|\s*`(--type-[\w-]+)`\s*\|\s*(\d+)\s*\/\s*(\d+)\s*\|\s*(\d+)\s*\|/g,
      ),
    ];
    expect(rows).toHaveLength(8);
    for (const [, name, size, line, weight] of rows) {
      expect(normalise(token(name as string)), name).toBe(
        normalise(`${weight} ${size}px / ${line}px var(--font-sans)`),
      );
    }
  });

  it('matches the radius scale and squircle multipliers (§5.1)', () => {
    const rows = [
      ...section(/^## 5\. Geometry/m).matchAll(/\|\s*`(--r-[\w-]+)`\s*\|\s*(\d+)\s*\|/g),
    ];
    expect(rows).toHaveLength(6);
    for (const [, name, px] of rows) expect(token(name as string), name).toBe(`${px}px`);

    for (const name of ['--corner-k', '--corner-k-sm']) {
      const rule = blocks.find((block) => block.prelude === `@property ${name}`);
      expect(rule?.body, name).toMatch(/inherits:\s*false/);
      expect(rule?.body, name).toMatch(/initial-value:\s*1\s*;/);
    }
    const supports = blocks.find((block) => block.prelude === '@supports (corner-shape: squircle)');
    const sq = supports
      ? topLevelBlocks(supports.body).find((b) => b.prelude === '.sq')
      : undefined;
    expect(sq?.body).toMatch(/corner-shape:\s*squircle/);
    expect(sq && declarations(sq.body).get('--corner-k')).toBe('1.6');
    expect(sq && declarations(sq.body).get('--corner-k-sm')).toBe('1.35');
  });

  it('matches the spacing grid (§5.2)', () => {
    const pairs = [...section(/^### 5\.2 Spacing/m).matchAll(/`(--s-[\d_]+):\s*(\d+)`/g)];
    expect(pairs).toHaveLength(11);
    for (const [, name, px] of pairs) expect(token(name as string), name).toBe(`${px}px`);
    expect(token('--island-pad')).toBe('16px');
    expect(token('--island-pad-dense')).toBe('12px');
    expect(token('--gutter')).toBe('8px');
  });

  it('matches control sizes, with a 44 px minimum under a coarse pointer (§5.3)', () => {
    expect([token('--ctl-sm'), token('--ctl-md'), token('--ctl-lg'), token('--ctl-xl')]).toEqual([
      '28px',
      '32px',
      '36px',
      '40px',
    ]);
    const coarse = mediaRoot('(pointer: coarse)');
    for (const name of ['--ctl-sm', '--ctl-md', '--ctl-lg', '--ctl-xl', '--target-min']) {
      expect(coarse.get(name), name).toBe('44px');
    }
  });

  it('matches elevation (§5.4)', () => {
    expect(normalise(token('--float-edge'))).toBe(normalise('0 0 0 1px rgb(255 255 255 / .06)'));
    expect(normalise(token('--float-shadow'))).toBe(normalise('0 12px 32px -12px rgb(0 0 0 / .5)'));
  });

  it('matches the z-index ladder (§5.5)', () => {
    const ladder = section(/^### 5\.5 Z-index/m).match(/`(raised[^`]+)`/)?.[1] ?? '';
    const steps = [...ladder.matchAll(/([a-z]+) (\d+)/g)];
    expect(steps).toHaveLength(9);
    for (const [, name, value] of steps) expect(token(`--z-${name}`), name).toBe(value);
  });

  it('matches motion durations and easings (§14)', () => {
    const motion = section(/^## 14\. Motion/m);
    for (const [, name, ms] of motion.matchAll(/\|\s*`(--motion-[\w-]+)`\s*\|\s*(\d+) ms/g)) {
      expect(token(name as string), name).toBe(`${ms}ms`);
    }
    expect(token('--motion-panel-exit')).toBe('180ms');
    for (const [, name, value] of motion.matchAll(/\|\s*`(--ease-[\w-]+)`\s*\|\s*`([^`]+)`/g)) {
      expect(normalise(token(name as string)), name).toBe(normalise(value as string));
    }
    expect(token('--motion-camera')).toBe('1200ms');
  });
});

describe('contrast (§3, WCAG 2.2)', () => {
  const pairs: ReadonlyArray<[fg: string, bg: string, min: number]> = [
    ['--ink', '--island', 15],
    ['--ink-2', '--island', 7],
    ['--ink-3', '--island', 4.5],
    ['--accent-ink', '--island', 8],
    ['--accent-ink', '--ground', 9],
    ['--on-accent', '--accent', 4.5],
    ['--on-accent', '--accent-press', 4.5],
    ['--on-accent', '--accent-hover', 4.5],
    ['--ink-3', '--popover', 4.5],
    ['--ink-3', '--control', 4.5],
    ['--alert', '--control', 4.5],
    ['--alert', '--popover', 4.5],
    ['--on-accent', '--alert-fill', 4.5],
  ];

  it.each(pairs)('%s on %s ≥ %d:1', (fg, bg, min) => {
    expect(contrast(hexOf(fg), hexOf(bg))).toBeGreaterThanOrEqual(min);
  });

  it('keeps every status and map colour at 3:1 or more on island and ground (non-text)', () => {
    const marks = [
      '--good',
      '--warning',
      '--alert',
      '--map-air',
      '--map-hazard',
      '--map-maritime',
      '--map-space',
      '--map-context',
      '--map-selection',
    ];
    for (const mark of marks) {
      for (const surface of ['--island', '--ground']) {
        expect(
          contrast(hexOf(mark), hexOf(surface)),
          `${mark} on ${surface}`,
        ).toBeGreaterThanOrEqual(3);
      }
    }
  });
});

describe('accessibility swaps (§13, §14)', () => {
  it('reduced motion zeroes every duration, keeping only 80 ms crossfades, and resets scale and distance', () => {
    const reduce = mediaRoot('(prefers-reduced-motion: reduce)');
    const motionTokens = [...base.keys()].filter((name) => name.startsWith('--motion-'));
    expect(motionTokens.length).toBeGreaterThanOrEqual(7);
    for (const name of motionTokens) {
      const value = reduce.get(name);
      expect(value, name).toBeDefined();
      expect(Number.parseFloat(value ?? 'NaN'), name).toBeLessThanOrEqual(80);
    }
    for (const name of ['--motion-press', '--motion-fast', '--motion-base', '--motion-camera']) {
      expect(reduce.get(name), name).toBe('0ms');
    }
    expect(reduce.get('--motion-panel')).toBe('80ms');
    expect(reduce.get('--scale-press')).toBe('1');
    for (const name of [...base.keys()].filter((n) => n.startsWith('--dist-'))) {
      expect(reduce.get(name), name).toBe('0px');
    }
  });

  it('more contrast promotes hairlines and tertiary ink', () => {
    const more = mediaRoot('(prefers-contrast: more)');
    expect(more.get('--hairline')).toBe(token('--hairline-strong'));
    expect(more.get('--ink-3')).toBe(token('--ink-2'));
  });

  it('forced colours maps surfaces and ink to system colours', () => {
    const forced = mediaRoot('(forced-colors: active)');
    expect(forced.get('--island')).toBe('Canvas');
    expect(forced.get('--ink')).toBe('CanvasText');
    expect(forced.get('--accent')).toBe('Highlight');
    expect(forced.get('--focus-ring-color')).toBe('Highlight');
  });
});
