/**
 * The command registry: one list of named actions that powers the palette's Actions group,
 * the global keyboard shortcuts and the shortcut sheet (design-system §7 "the same registry
 * powers shortcuts"). Later it also backs voice and MCP (product spec §4.3), which is why a
 * command is data plus a handler over the runtime, not a closure inside a component.
 */
import type { Runtime } from '../../runtime';

export interface Shortcut {
  /** `KeyboardEvent.key` values that trigger it; letters are compared in lower case. */
  readonly keys: readonly string[];
  /** Requires ⌘ (macOS) or Ctrl (elsewhere). */
  readonly mod?: boolean;
  /** How the key is written, without the modifier: "K", "/", "+". */
  readonly label: string;
}

export interface CommandContext {
  readonly runtime: Runtime;
  /** Moves focus to the search field; false when it is not on screen (narrow layouts). */
  readonly focusSearch: () => boolean;
}

export interface Command {
  readonly id: string;
  /** Sentence case, a plain verb where there is one. */
  readonly title: string;
  readonly keywords: readonly string[];
  readonly shortcut?: Shortcut;
  /** Listed in the palette's Actions group (opening the palette itself is not). */
  readonly inPalette: boolean;
  readonly isEnabled?: (ctx: CommandContext) => boolean;
  readonly run: (ctx: CommandContext) => void;
}

const ui = (ctx: CommandContext) => ctx.runtime.stores.ui.getState();
const globe = (ctx: CommandContext) => ui(ctx).globe;
const hasGlobe = (ctx: CommandContext) => globe(ctx) !== null;

export const COMMANDS: readonly Command[] = [
  {
    id: 'palette.toggle',
    title: 'Open the command palette',
    keywords: ['commands', 'search', 'palette'],
    shortcut: { keys: ['k'], mod: true, label: 'K' },
    inPalette: false,
    run: (ctx) => {
      const state = ui(ctx);
      state.setShortcutsOpen(false);
      state.setPaletteOpen(!state.paletteOpen);
    },
  },
  {
    id: 'search.focus',
    title: 'Search places, coordinates and callsigns',
    keywords: ['find', 'search'],
    shortcut: { keys: ['/'], label: '/' },
    inPalette: false,
    run: (ctx) => {
      if (!ctx.focusSearch()) ui(ctx).setPaletteOpen(true);
    },
  },
  {
    id: 'globe.zoomIn',
    title: 'Zoom in',
    keywords: ['closer', 'magnify'],
    shortcut: { keys: ['+', '='], label: '+' },
    inPalette: true,
    isEnabled: hasGlobe,
    run: (ctx) => globe(ctx)?.zoomIn(),
  },
  {
    id: 'globe.zoomOut',
    title: 'Zoom out',
    keywords: ['further', 'away'],
    shortcut: { keys: ['-', '_'], label: '−' },
    inPalette: true,
    isEnabled: hasGlobe,
    run: (ctx) => globe(ctx)?.zoomOut(),
  },
  {
    id: 'globe.resetNorth',
    title: 'Reset north',
    keywords: ['north up', 'compass', 'rotate', 'orient'],
    shortcut: { keys: ['n'], label: 'N' },
    inPalette: true,
    isEnabled: hasGlobe,
    run: (ctx) => globe(ctx)?.resetNorth(),
  },
  {
    id: 'globe.home',
    title: 'Home',
    keywords: ['whole globe', 'reset view', 'overview'],
    shortcut: { keys: ['h'], label: 'H' },
    inPalette: true,
    isEnabled: hasGlobe,
    run: (ctx) => globe(ctx)?.home(),
  },
  {
    id: 'panel.layers',
    title: 'Show layers',
    keywords: ['layers', 'explorer', 'feeds'],
    inPalette: true,
    run: (ctx) => ui(ctx).setSidePanel('layers'),
  },
  {
    id: 'panel.sources',
    title: 'Show sources',
    keywords: ['sources', 'attribution', 'credits', 'licence', 'health'],
    inPalette: true,
    run: (ctx) => ui(ctx).setSidePanel('sources'),
  },
  {
    id: 'selection.clear',
    title: 'Clear the selection',
    keywords: ['deselect', 'close inspector'],
    shortcut: { keys: ['Escape'], label: 'Esc' },
    inPalette: true,
    isEnabled: (ctx) => ctx.runtime.stores.workspace.getState().selection !== null,
    run: (ctx) => ctx.runtime.stores.workspace.getState().select(null),
  },
  {
    id: 'shortcuts.show',
    title: 'Show keyboard shortcuts',
    keywords: ['keys', 'help', 'hotkeys'],
    shortcut: { keys: ['?'], label: '?' },
    inPalette: true,
    run: (ctx) => {
      const state = ui(ctx);
      state.setPaletteOpen(false);
      state.setShortcutsOpen(true);
    },
  },
];

/** Whether this keyboard is a Mac's, where the modifier is ⌘ rather than Ctrl. */
export function isApplePlatform(): boolean {
  if (typeof navigator === 'undefined') return false;
  return /Mac|iPhone|iPad|iPod/.test(navigator.platform || navigator.userAgent);
}

/** "⌘K" on Apple keyboards, "Ctrl K" elsewhere; "/" for a bare key. */
export function shortcutLabel(shortcut: Shortcut, apple = isApplePlatform()): string {
  if (!shortcut.mod) return shortcut.label;
  return apple ? `⌘${shortcut.label}` : `Ctrl ${shortcut.label}`;
}

/** Whether a key event triggers `shortcut`. Shift is ignored so `?` and `+` work everywhere. */
export function matchesShortcut(
  event: Pick<KeyboardEvent, 'key' | 'metaKey' | 'ctrlKey' | 'altKey'>,
  shortcut: Shortcut,
): boolean {
  if (event.altKey) return false;
  const mod = event.metaKey || event.ctrlKey;
  if (Boolean(shortcut.mod) !== mod) return false;
  const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
  return shortcut.keys.includes(key);
}

/** Case-insensitive match of a palette query against a command's title and keywords. */
export function matchesQuery(command: Pick<Command, 'title' | 'keywords'>, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (q === '') return true;
  return [command.title, ...command.keywords].some((text) => text.toLowerCase().includes(q));
}

export function commandById(id: string): Command | undefined {
  return COMMANDS.find((c) => c.id === id);
}
