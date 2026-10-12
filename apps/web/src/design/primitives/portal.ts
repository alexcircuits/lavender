import { createContext, type RefObject, useContext } from 'react';

/**
 * Where popups (menus, popovers, tooltips, dialogs) portal to. DesignProvider renders the
 * container inside `.lv-root`, so portaled content gets the scoped base styles; without a
 * provider (isolated tests) Base UI falls back to document.body.
 */
export const PortalContainerContext = createContext<RefObject<HTMLDivElement | null> | null>(null);

/** Props to spread onto a Base UI `*.Portal`. */
export function usePortalProps(): { container?: RefObject<HTMLDivElement | null> } {
  const container = useContext(PortalContainerContext);
  return container ? { container } : {};
}
