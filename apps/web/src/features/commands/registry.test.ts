import { describe, expect, it } from 'vitest';
import { createRuntime } from '../../runtime';
import { FakePlatform, fakeGlobe } from '../../test/fakes';
import { COMMANDS, commandById, matchesQuery, matchesShortcut, shortcutLabel } from './registry';
import { runCommand } from './useCommands';

const key = (k: string, mods: Partial<Record<'metaKey' | 'ctrlKey' | 'altKey', boolean>> = {}) => ({
  key: k,
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  ...mods,
});

describe('command registry', () => {
  it('has unique ids and unique shortcuts', () => {
    const ids = COMMANDS.map((c) => c.id);
    expect(new Set(ids).size).toBe(ids.length);
    const keys = COMMANDS.flatMap((c) =>
      (c.shortcut?.keys ?? []).map((k) => `${c.shortcut?.mod ? 'mod+' : ''}${k}`),
    );
    expect(new Set(keys).size).toBe(keys.length);
  });

  it('matches ⌘K and Ctrl K, but not a bare K', () => {
    const palette = commandById('palette.toggle')?.shortcut;
    if (!palette) throw new Error('palette shortcut');
    expect(matchesShortcut(key('k', { metaKey: true }), palette)).toBe(true);
    expect(matchesShortcut(key('K', { ctrlKey: true }), palette)).toBe(true);
    expect(matchesShortcut(key('k'), palette)).toBe(false);
    expect(matchesShortcut(key('k', { metaKey: true, altKey: true }), palette)).toBe(false);
  });

  it('matches bare keys in either case, never with a modifier', () => {
    const north = commandById('globe.resetNorth')?.shortcut;
    if (!north) throw new Error('north shortcut');
    expect(matchesShortcut(key('n'), north)).toBe(true);
    expect(matchesShortcut(key('N'), north)).toBe(true);
    expect(matchesShortcut(key('n', { ctrlKey: true }), north)).toBe(false);
  });

  it('writes the modifier for the platform', () => {
    const palette = commandById('palette.toggle')?.shortcut;
    if (!palette) throw new Error('palette shortcut');
    expect(shortcutLabel(palette, true)).toBe('⌘K');
    expect(shortcutLabel(palette, false)).toBe('Ctrl K');
  });

  it('finds commands by title or keyword', () => {
    const home = commandById('globe.home');
    if (!home) throw new Error('home');
    expect(matchesQuery(home, 'whole globe')).toBe(true);
    expect(matchesQuery(home, 'HOME')).toBe(true);
    expect(matchesQuery(home, 'zzz')).toBe(false);
  });

  it('runs only enabled commands', () => {
    const runtime = createRuntime(new FakePlatform());
    const ctx = { runtime, focusSearch: () => false };
    const home = commandById('globe.home');
    if (!home) throw new Error('home');
    expect(runCommand(home, ctx)).toBe(false);
    const { globe } = fakeGlobe();
    runtime.stores.ui.getState().setGlobe(globe);
    expect(runCommand(home, ctx)).toBe(true);
    expect(globe.home).toHaveBeenCalledOnce();

    const search = commandById('search.focus');
    if (!search) throw new Error('search');
    runCommand(search, ctx);
    // No search field on screen: the palette opens instead.
    expect(runtime.stores.ui.getState().paletteOpen).toBe(true);
  });
});
