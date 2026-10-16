import { type RefObject, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { useUi, useWorkspace } from '../runtime';
import type { SidePanel } from '../state/stores';
import { useMediaQuery } from './useMediaQuery';

/**
 * Below this width the layers island and the inspector together would cover more than 60 % of
 * the globe (design-system §12): rail 56 + gutters 3 × 8 + 320 + 360 = 760 px of chrome, and
 * 704 / (W − 56) > 0.6 below W ≈ 1 190 px. There, the surface opened last wins.
 */
export const SHARE_BREAKPOINT_PX = 1190;

export type Surface = 'side' | 'inspector';

interface Opened {
  readonly side: SidePanel | null;
  readonly selected: boolean;
  readonly last: Surface;
}

function lastOpened(previous: Opened, side: SidePanel | null, selected: boolean): Surface {
  if (selected && !previous.selected) return 'inspector';
  if (side !== null && side !== previous.side) return 'side';
  return previous.last;
}

/**
 * Which of the two islands to fold away when they cannot share the screen, or null. Derived
 * while rendering (not in an effect), so a new inspector is never painted hidden for a frame.
 */
export function useCollapsed(): Surface | null {
  const sidePanel = useUi((s) => s.sidePanel);
  const selected = useWorkspace((s) => s.selection !== null);
  const crowded = useMediaQuery(`(max-width: ${SHARE_BREAKPOINT_PX - 1}px)`);
  const [opened, setOpened] = useState<Opened>({ side: sidePanel, selected, last: 'side' });
  let last = opened.last;
  if (opened.side !== sidePanel || opened.selected !== selected) {
    last = lastOpened(opened, sidePanel, selected);
    setOpened({ side: sidePanel, selected, last });
  }
  if (!crowded || sidePanel === null || !selected) return null;
  return last === 'inspector' ? 'side' : 'inspector';
}

/**
 * The rail button for a panel (NavRail marks them), where focus goes when its island closes.
 * The launch list has none of its own: it is opened from, and belongs to, the Layers island.
 */
export const railButton = (panel: SidePanel) =>
  document.querySelector<HTMLElement>(
    `[data-rail-panel="${panel === 'launches' ? 'layers' : panel}"]`,
  );

/**
 * When the left island closes or switches while it held focus (its Close button, "Set up in
 * Sources", the footer's Sources), focus would fall to <body>. Send it to the rail button of
 * the panel now shown, or of the one that closed (design-system §13: focus returns to triggers).
 */
export function useRailFocusReturn(container: RefObject<HTMLElement | null>): void {
  const sidePanel = useUi((s) => s.sidePanel);
  const lastFocused = useRef<HTMLElement | null>(null);
  const previousPanel = useRef(sidePanel);

  useEffect(() => {
    const element = container.current;
    if (!element) return;
    const onFocusIn = (event: FocusEvent) => {
      if (event.target instanceof HTMLElement) lastFocused.current = event.target;
    };
    const onFocusOut = (event: FocusEvent) => {
      // Focus moving somewhere real outside the island: nothing to restore later.
      const next = event.relatedTarget;
      if (next instanceof Node && !element.contains(next)) lastFocused.current = null;
    };
    element.addEventListener('focusin', onFocusIn);
    element.addEventListener('focusout', onFocusOut);
    return () => {
      element.removeEventListener('focusin', onFocusIn);
      element.removeEventListener('focusout', onFocusOut);
    };
  }, [container]);

  useLayoutEffect(() => {
    const before = previousPanel.current;
    previousPanel.current = sidePanel;
    const focused = lastFocused.current;
    if (before === sidePanel || !focused || focused.isConnected) return;
    const active = document.activeElement;
    if (active && active !== document.body && active.isConnected) return;
    lastFocused.current = null;
    const target = sidePanel ?? before;
    if (target) railButton(target)?.focus();
  }, [sidePanel]);
}
