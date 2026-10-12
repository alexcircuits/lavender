import { type ReactNode, useRef } from 'react';
import { PortalContainerContext } from './portal';
import { Toaster } from './Toast';
import { TooltipProvider } from './Tooltip';

export interface DesignProviderProps {
  children: ReactNode;
}

/**
 * Shared tooltip timing, the toast viewport and the popup portal container. Mount once, at the
 * app root (inside `.lv-root`).
 */
export function DesignProvider({ children }: DesignProviderProps) {
  const portalRef = useRef<HTMLDivElement | null>(null);
  return (
    <PortalContainerContext.Provider value={portalRef}>
      <TooltipProvider>
        {children}
        <Toaster />
        <div ref={portalRef} className="lv-portals" />
      </TooltipProvider>
    </PortalContainerContext.Provider>
  );
}
