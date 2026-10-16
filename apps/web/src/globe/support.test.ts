import { Color } from '@cesium/core';
import { describe, expect, it, vi } from 'vitest';
import { CONTEXT_LOST_MESSAGE, watchContextLoss } from './contextLoss';
import { entityFromPick, isEntityRef, sameEntity } from './entityRef';
import { Listeners } from './listeners';
import { toMapColors } from './renderers/types';
import { cssTokenReader, DEFAULT_PALETTE, readPalette } from './tokens';

describe('map tokens', () => {
  it('reads the symbology tokens, falling back to the validated values', () => {
    const tokens: Record<string, string> = { '--map-air': '#123456', '--hazard-5': '#ffffff' };
    const palette = readPalette((name) => tokens[name] ?? '');
    expect(palette.air).toBe('#123456');
    expect(palette.hazardRamp).toEqual([...DEFAULT_PALETTE.hazardRamp.slice(0, 4), '#ffffff']);
    expect(palette.selection).toBe(DEFAULT_PALETTE.selection);
    expect(palette.labelFont).toBe(DEFAULT_PALETTE.labelFont);
  });

  it('reads custom properties from computed CSS', () => {
    document.documentElement.style.setProperty('--map-space', '#abcdef');
    try {
      expect(readPalette(cssTokenReader()).space).toBe('#abcdef');
    } finally {
      document.documentElement.style.removeProperty('--map-space');
    }
  });

  it('falls back when a token is a colour the WebGL side cannot parse (forced colours)', () => {
    const colors = toMapColors({
      ...DEFAULT_PALETTE,
      selection: 'Highlight',
      air: 'oklch(0.6 0.1 250)',
    });
    expect(
      Color.equals(colors.selection, Color.fromCssColorString(DEFAULT_PALETTE.selection)),
    ).toBe(true);
    expect(colors.css.selection).toBe(DEFAULT_PALETTE.selection);
    expect(colors.css.air).toBe(DEFAULT_PALETTE.air);
  });
});

describe('entity refs', () => {
  it('recognises EntityRef ids in pick results', () => {
    expect(entityFromPick({ primitive: {}, id: { feed: 'aircraft', id: 'abc123' } })).toEqual({
      feed: 'aircraft',
      id: 'abc123',
    });
    expect(entityFromPick(undefined)).toBeNull();
    expect(entityFromPick({ id: 'globe' })).toBeNull();
    expect(entityFromPick({ id: { feed: 'ships', id: '1' } })).toBeNull();
    expect(isEntityRef({ feed: 'satellites', id: '' })).toBe(false);
  });

  it('recognises picked vessel chevrons, dots and trails (ENG-980)', () => {
    expect(entityFromPick({ primitive: {}, id: { feed: 'vessels', id: '230990001' } })).toEqual({
      feed: 'vessels',
      id: '230990001',
    });
  });

  it('compares refs by value', () => {
    expect(sameEntity({ feed: 'aircraft', id: 'a' }, { feed: 'aircraft', id: 'a' })).toBe(true);
    expect(sameEntity({ feed: 'aircraft', id: 'a' }, null)).toBe(false);
    expect(sameEntity(null, null)).toBe(true);
  });
});

describe('listeners', () => {
  it('unsubscribes, survives a failing listener and clears', () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    const listeners = new Listeners<number>('test');
    const seen: number[] = [];
    const off = listeners.add((n) => seen.push(n));
    listeners.add(() => {
      throw new Error('boom');
    });
    listeners.emit(1);
    off();
    off();
    listeners.emit(2);
    expect(seen).toEqual([1]);
    expect(error).toHaveBeenCalledTimes(2);
    listeners.clear();
    expect(listeners.size).toBe(0);
  });
});

describe('WebGL context loss', () => {
  it('reports a lost context as a render error and logs a restore', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const canvas = document.createElement('canvas');
    const onLost = vi.fn();
    const onRestored = vi.fn();
    const stop = watchContextLoss(canvas, { onLost, onRestored });

    const lost = new Event('webglcontextlost', { cancelable: true });
    canvas.dispatchEvent(lost);
    expect(lost.defaultPrevented).toBe(true); // allows the browser to restore the context
    expect(onLost).toHaveBeenCalledWith(new Error(CONTEXT_LOST_MESSAGE));

    canvas.dispatchEvent(new Event('webglcontextrestored'));
    expect(onRestored).toHaveBeenCalledTimes(1);
    expect(warn).toHaveBeenCalledTimes(1);

    stop();
    canvas.dispatchEvent(new Event('webglcontextlost', { cancelable: true }));
    expect(onLost).toHaveBeenCalledTimes(1);
  });
});
