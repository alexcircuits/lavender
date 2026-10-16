/**
 * Launch pads against real Cesium collections (no GPU until a frame is drawn): one billboard
 * per pad, kept in place by pad id, a count badge for busy pads, the pick target, the halo
 * position for a pad or any of its launches, stale dimming, the label budget, and no render
 * request when nothing changed.
 */
import { Cartesian3, Color } from '@cesium/core';
import {
  type Billboard,
  BillboardCollection,
  type Label,
  LabelCollection,
  PrimitiveCollection,
} from '@cesium/engine';
import { describe, expect, it, vi } from 'vitest';
import type { FeedSnapshot, Freshness, Launch } from '../../contracts';
import { padIdOf } from '../../lib/launches';
import { CAPE_PAD, launch, launchSnapshot, padlessLaunch, WEST_PAD } from '../../test/launch';
import { DEFAULT_FONT_SANS, DEFAULT_PALETTE } from '../tokens';
import { GLYPH_SUPERSAMPLE } from './glyphs';
import {
  badgeFont,
  drawPadGlyph,
  LaunchPadRenderer,
  PAD_BADGE,
  PAD_GLYPH_PX,
  PAD_LABEL_BUDGET,
  PAD_SYMBOL_PX,
  padBadge,
  padGlyphId,
  padLabel,
} from './launches';
import { STALE_ALPHA } from './symbology';
import { type RenderContext, toMapColors } from './types';

const CAPE = padIdOf(CAPE_PAD);
const WEST = padIdOf(WEST_PAD);

function context(heightM = 5_000_000) {
  const camera = { heightM, position: Cartesian3.fromDegrees(-80, 28, heightM) };
  const requestRender = vi.fn();
  const ctx: RenderContext = {
    colors: toMapColors(DEFAULT_PALETTE),
    requestRender,
    cameraPosition: () => camera.position,
    cameraHeightM: () => camera.heightM,
  };
  return { ctx, camera, requestRender };
}

function setup(heightM?: number) {
  const primitives = new PrimitiveCollection();
  const { ctx, camera, requestRender } = context(heightM);
  const renderer = new LaunchPadRenderer(primitives, ctx);
  return { primitives, renderer, ctx, camera, requestRender };
}

function collection<T>(primitives: PrimitiveCollection, kind: new () => T): T {
  for (let i = 0; i < primitives.length; i += 1) {
    const candidate = primitives.get(i);
    if (candidate instanceof kind) return candidate;
  }
  throw new Error(`no ${kind.name}`);
}

function billboardsOf(primitives: PrimitiveCollection): Billboard[] {
  const billboards = collection(primitives, BillboardCollection);
  return Array.from({ length: billboards.length }, (_, i) => billboards.get(i));
}

function labelsOf(primitives: PrimitiveCollection): Label[] {
  const labels = collection(primitives, LabelCollection);
  return Array.from({ length: labels.length }, (_, i) => labels.get(i));
}

const pads = (items: Launch[], freshness?: Freshness): FeedSnapshot =>
  launchSnapshot(items, freshness ? { freshness } : {});

describe('pad glyph', () => {
  it('badges busy pads with their count, capped at 9+, one atlas entry per badge', () => {
    expect(padBadge(1)).toBeNull();
    expect(padBadge(0)).toBeNull();
    expect(padBadge(2)).toBe('2');
    expect(padBadge(9)).toBe('9');
    expect(padBadge(14)).toBe('9+');
    expect(padGlyphId(1)).toBe('lv-launch-pad-1');
    expect(padGlyphId(3)).toBe('lv-launch-pad-3');
    expect(padGlyphId(40)).toBe(padGlyphId(10));
  });

  it('is drawn at the supersampled glyph size, its count in the label face', () => {
    const canvas = drawPadGlyph(3, DEFAULT_PALETTE.outline, DEFAULT_PALETTE.labelFont);
    expect(canvas.width).toBe(PAD_GLYPH_PX * GLYPH_SUPERSAMPLE);
    expect(canvas.height).toBe(canvas.width);
    expect(badgeFont(DEFAULT_PALETTE.labelFont, '3')).toBe(`600 9px ${DEFAULT_FONT_SANS}`);
    expect(badgeFont('', '9+')).toBe(`600 8px ${DEFAULT_FONT_SANS}`);
  });

  it('writes the count large enough to read at 1x: a 9 px digit in an 11 px disc', () => {
    // 7.5 px digits could not be told apart at standard density (review of the 1x screenshots).
    const px = (font: string) => Number(/(\d+(?:\.\d+)?)px/.exec(font)?.[1]);
    expect(px(badgeFont(DEFAULT_PALETTE.labelFont, '6'))).toBeGreaterThanOrEqual(9);
    expect(px(badgeFont(DEFAULT_PALETTE.labelFont, '9+'))).toBeGreaterThanOrEqual(8);
    expect(PAD_BADGE.radius * 2).toBeGreaterThanOrEqual(11);
  });

  it('keeps the badge inside the selection halo, so a selected busy pad still shows its count', () => {
    // drawHalo's innermost ring starts at ceil(symbolPx) / 2 + 1 from the centre.
    const haloInner = Math.ceil(PAD_SYMBOL_PX) / 2 + 1;
    const centre = PAD_GLYPH_PX / 2;
    const badgeReach = Math.hypot(PAD_BADGE.x - centre, PAD_BADGE.y - centre) + PAD_BADGE.radius;
    expect(badgeReach).toBeLessThanOrEqual(haloInner);
    expect(PAD_BADGE.x + PAD_BADGE.radius).toBeLessThanOrEqual(PAD_GLYPH_PX);
    expect(PAD_BADGE.y - PAD_BADGE.radius).toBeGreaterThanOrEqual(0);
  });

  it('labels pads by name, shortened for the globe', () => {
    expect(padLabel({ name: ' Space Launch Complex 40 ' })).toBe('Space Launch Complex 40');
    const long = padLabel({ name: 'Launch Complex 39A / Launch Complex 39B (shared)' });
    expect(long).toHaveLength(28);
    expect(long.endsWith('…')).toBe(true);
  });
});

describe('LaunchPadRenderer', () => {
  it('draws one Space-family billboard per pad and keeps it in place across snapshots', () => {
    const { primitives, renderer, ctx, requestRender } = setup();
    renderer.apply(pads([launch('a'), launch('b', { pad: WEST_PAD }), launch('c')]));
    expect(renderer.size).toBe(2);
    const [cape, west] = billboardsOf(primitives);
    expect(cape?.image).toBe(padGlyphId(2));
    expect(west?.image).toBe(padGlyphId(1));
    expect(Color.equals(cape?.color, ctx.colors.space)).toBe(true);
    expect(cape?.scale).toBe(1 / GLYPH_SUPERSAMPLE);
    expect(requestRender).toHaveBeenCalledTimes(1);

    // The Cape loses a launch and the western pad goes: same Cape billboard, no badge.
    renderer.apply(pads([launch('a')]));
    const after = billboardsOf(primitives);
    expect(after).toEqual([cape]);
    expect(cape?.image).toBe(padGlyphId(1));
  });

  it('picks a single-launch pad as its launch and a busy pad as the pad', () => {
    const { primitives, renderer } = setup();
    renderer.apply(pads([launch('a'), launch('b', { pad: WEST_PAD }), launch('c')]));
    const [cape, west] = billboardsOf(primitives);
    expect(cape?.id).toEqual({ feed: 'launches', id: CAPE });
    expect(west?.id).toEqual({ feed: 'launches', id: 'b' });
    renderer.apply(pads([launch('c'), launch('b', { pad: WEST_PAD })]));
    expect(cape?.id).toEqual({ feed: 'launches', id: 'c' });
  });

  it('locates a pad by its id or by any of its launches, for the selection halo', () => {
    const { renderer } = setup();
    renderer.apply(pads([launch('a'), launch('c'), launch('b', { pad: WEST_PAD })]));
    const expected = Cartesian3.fromDegrees(CAPE_PAD.longitude, CAPE_PAD.latitude, 0);
    expect(renderer.locate(CAPE)?.position).toEqual(expected);
    expect(renderer.locate('c')).toEqual({ position: expected, symbolPx: PAD_SYMBOL_PX });
    expect(renderer.locate(WEST)).toBeDefined();
    expect(renderer.locate('missing')).toBeUndefined();
    expect(renderer.locate('pad:1.0000,2.0000')).toBeUndefined();
    renderer.clear();
    expect(renderer.locate('a')).toBeUndefined();
  });

  it('skips launches whose pad cannot be placed, and snapshots of other feeds', () => {
    const { renderer } = setup();
    renderer.apply(
      pads([
        launch('ok'),
        padlessLaunch('no-pad'),
        launch('bad', { pad: { ...WEST_PAD, latitude: Number.NaN } }),
        launch('far', { pad: { ...WEST_PAD, longitude: 200 } }),
        // Malformed on the wire: a pad with a name that is not text, a launch with no status.
        launch('odd', { pad: { ...WEST_PAD, name: 4 as unknown as string } }),
        { ...launch('nostatus', { pad: WEST_PAD }), status: undefined as unknown as string },
      ]),
    );
    expect(renderer.size).toBe(1);
    expect(renderer.locate('odd')).toBeUndefined();
    renderer.apply({ ...pads([]), feed: 'earthquakes', items: { kind: 'earthquakes', items: [] } });
    expect(renderer.size).toBe(1);
  });

  it('dims pads and labels to 60 % while the feed is stale, and back when live', () => {
    const { primitives, renderer } = setup(200_000);
    renderer.apply(pads([launch('a')], { state: 'stale', ageS: 7_200 }));
    const [billboard] = billboardsOf(primitives);
    const [label] = labelsOf(primitives);
    expect(billboard?.color.alpha).toBeCloseTo(STALE_ALPHA, 9);
    expect(label?.fillColor.alpha).toBeCloseTo(STALE_ALPHA, 9);
    expect(
      Color.equals(billboard?.color.withAlpha(1), Color.fromCssColorString(DEFAULT_PALETTE.space)),
    ).toBe(true);
    renderer.apply(pads([launch('a')]));
    expect(billboard?.color.alpha).toBe(1);
    expect(label?.fillColor.alpha).toBe(1);
    // Unavailable with the last data kept is drawn stale too.
    renderer.apply(pads([launch('a')], { state: 'unavailable', reason: 'down' }));
    expect(billboard?.color.alpha).toBeCloseTo(STALE_ALPHA, 9);
  });

  it('labels the nearest pads below the label height, always the selected one', () => {
    const { primitives, renderer, camera } = setup(500_000);
    const many = Array.from({ length: PAD_LABEL_BUDGET + 3 }, (_, i) =>
      launch(`l${i}`, { pad: { ...CAPE_PAD, name: `Pad ${i}`, longitude: -80 + i * 0.5 } }),
    );
    renderer.apply(pads(many));
    expect(renderer.labelledIds()).toHaveLength(PAD_LABEL_BUDGET);
    const far = padIdOf({ latitude: CAPE_PAD.latitude, longitude: -80 + (many.length - 1) * 0.5 });
    expect(renderer.labelledIds()).not.toContain(far);

    // Selecting the far pad's launch labels its pad.
    renderer.setSelected(`l${many.length - 1}`);
    expect(renderer.labelledIds()).toContain(far);
    expect(labelsOf(primitives).map((l) => l.text)).toContain(`Pad ${many.length - 1}`);

    camera.heightM = 5_000_000;
    renderer.viewChanged();
    expect(renderer.labelledIds()).toEqual([far]);
    renderer.setSelected(null);
    expect(renderer.labelledIds()).toEqual([]);
  });

  it('asks for a frame only when something it draws changed', () => {
    const { renderer, requestRender } = setup(5_000_000);
    renderer.apply(pads([launch('a')]));
    requestRender.mockClear();
    renderer.viewChanged();
    renderer.setSelected('a'); // a label above the label height: one change
    expect(requestRender).toHaveBeenCalledTimes(1);
    renderer.setSelected('a');
    renderer.viewChanged();
    expect(requestRender).toHaveBeenCalledTimes(1);
    renderer.clear();
    renderer.clear();
    expect(requestRender).toHaveBeenCalledTimes(2);
  });

  it('does not ask for a frame when a refresh brings the same pads', () => {
    // The server re-fetches on its TTL (every 30 s in fixture mode) with unchanged launches.
    const { renderer, requestRender } = setup(5_000_000);
    const items = () => [launch('a'), launch('b', { pad: WEST_PAD }), launch('c')];
    renderer.apply(pads(items()));
    expect(requestRender).toHaveBeenCalledTimes(1);
    renderer.apply({ ...pads(items()), generatedAt: '2026-06-02T11:59:31Z' });
    expect(requestRender).toHaveBeenCalledTimes(1);

    // Any change it draws asks for one: a launch more (badge), a pad gone, stale, a new name.
    renderer.apply(pads([...items(), launch('d')]));
    expect(requestRender).toHaveBeenCalledTimes(2);
    renderer.apply(pads([launch('a'), launch('c'), launch('d')]));
    expect(requestRender).toHaveBeenCalledTimes(3);
    renderer.apply(pads([launch('a'), launch('c'), launch('d')], { state: 'stale', ageS: 4_000 }));
    expect(requestRender).toHaveBeenCalledTimes(4);
    renderer.apply(
      pads([launch('a', { pad: { ...CAPE_PAD, name: 'Renamed' } }), launch('c'), launch('d')], {
        state: 'stale',
        ageS: 4_000,
      }),
    );
    expect(requestRender).toHaveBeenCalledTimes(5);
  });

  it('removes everything on clear and its collections on destroy', () => {
    const { primitives, renderer } = setup(200_000);
    renderer.apply(pads([launch('a'), launch('b', { pad: WEST_PAD })]));
    expect(labelsOf(primitives)).toHaveLength(2);
    renderer.clear();
    expect(renderer.size).toBe(0);
    expect(billboardsOf(primitives)).toHaveLength(0);
    expect(labelsOf(primitives)).toHaveLength(0);
    renderer.destroy();
    expect(primitives.length).toBe(0);
  });
});
