import { useEffect, useMemo } from 'react';
import { useRuntime } from '../../runtime';
import { COMMANDS, type Command, type CommandContext, matchesShortcut } from './registry';

/**
 * Marks the top search field's input so commands can focus it. A data attribute, not an id:
 * cmdk assigns its own input id for its ARIA wiring.
 */
export const SEARCH_INPUT_ATTRIBUTE = 'data-lv-search-input';
/** The id of the globe region; focus lands there when the surface that held it closes. */
export const GLOBE_REGION_ID = 'lv-globe';

function focusSearch(): boolean {
  const input = document.querySelector(`[${SEARCH_INPUT_ATTRIBUTE}]`);
  if (!(input instanceof HTMLInputElement)) return false;
  input.focus();
  return document.activeElement === input;
}

/** The context every command runs in. Stable for the runtime's lifetime. */
export function useCommandContext(): CommandContext {
  const runtime = useRuntime();
  return useMemo(() => ({ runtime, focusSearch }), [runtime]);
}

/** Runs a command if it is enabled; returns whether it ran. */
export function runCommand(command: Command, ctx: CommandContext): boolean {
  if (command.isEnabled && !command.isEnabled(ctx)) return false;
  command.run(ctx);
  return true;
}

function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName);
}

/** Inside an open menu, listbox or dialog the key belongs to that widget. */
function isInsidePopup(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return (
    target.closest('[role="menu"], [role="listbox"], [role="dialog"], [role="alertdialog"]') !==
    null
  );
}

/**
 * Binds every registry shortcut on `window`. Chords with ⌘/Ctrl work anywhere; bare keys are
 * ignored while typing, inside menus and dialogs, and while the palette or sheet is open.
 */
export function useGlobalShortcuts(ctx: CommandContext): void {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.isComposing) return;
      const command = COMMANDS.find((c) => c.shortcut && matchesShortcut(event, c.shortcut));
      if (!command?.shortcut) return;
      if (!command.shortcut.mod) {
        const ui = ctx.runtime.stores.ui.getState();
        if (ui.paletteOpen || ui.shortcutsOpen) return;
        if (isTypingTarget(event.target) || isInsidePopup(event.target)) return;
      }
      if (command.isEnabled && !command.isEnabled(ctx)) return;
      event.preventDefault();
      command.run(ctx);
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [ctx]);
}
