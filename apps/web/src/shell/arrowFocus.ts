import type { KeyboardEvent } from 'react';

const NEXT = new Set(['ArrowDown', 'ArrowRight']);
const PREVIOUS = new Set(['ArrowUp', 'ArrowLeft']);

/**
 * Arrow keys, Home and End move focus between the buttons of a rail or toolbar (design-system
 * §13). Every button stays in the Tab order too, so nothing depends on knowing the pattern.
 */
export function moveFocusWithArrows(event: KeyboardEvent<HTMLElement>): void {
  if (event.altKey || event.ctrlKey || event.metaKey) return;
  const buttons = [
    ...event.currentTarget.querySelectorAll<HTMLButtonElement>('button:not([disabled])'),
  ];
  const active = document.activeElement;
  const index = active instanceof HTMLButtonElement ? buttons.indexOf(active) : -1;
  if (index < 0) return;
  let next: HTMLButtonElement | undefined;
  if (NEXT.has(event.key)) next = buttons[(index + 1) % buttons.length];
  else if (PREVIOUS.has(event.key)) next = buttons[(index - 1 + buttons.length) % buttons.length];
  else if (event.key === 'Home') next = buttons[0];
  else if (event.key === 'End') next = buttons[buttons.length - 1];
  if (!next) return;
  event.preventDefault();
  next.focus();
}
