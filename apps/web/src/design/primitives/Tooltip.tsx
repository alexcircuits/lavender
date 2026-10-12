import { Tooltip as BaseTooltip } from '@base-ui/react/tooltip';
import type { ReactElement, ReactNode } from 'react';
import { Kbd } from './Kbd';
import { usePortalProps } from './portal';
import styles from './Tooltip.module.css';

/** Hover delay before a tooltip opens (design-system §7: "after 500 ms hover or on focus"). */
export const TOOLTIP_DELAY_MS = 500;

export interface TooltipProps {
  /** A label or a key hint — never essential information. */
  content: ReactNode;
  /** Optional key hint shown after the label, e.g. "⌘K". */
  shortcut?: string;
  side?: 'top' | 'bottom' | 'left' | 'right';
  /** The trigger: an element that forwards its ref and props (any primitive here does). */
  children: ReactElement;
  disabled?: boolean;
}

export function Tooltip({ content, shortcut, side = 'top', children, disabled }: TooltipProps) {
  const portal = usePortalProps();
  return (
    <BaseTooltip.Root disabled={disabled ?? false}>
      <BaseTooltip.Trigger delay={TOOLTIP_DELAY_MS} render={children} />
      <BaseTooltip.Portal {...portal}>
        <BaseTooltip.Positioner side={side} sideOffset={8} className={styles.positioner}>
          <BaseTooltip.Popup className={styles.popup}>
            <span>{content}</span>
            {shortcut ? <Kbd className={styles.kbd}>{shortcut}</Kbd> : null}
          </BaseTooltip.Popup>
        </BaseTooltip.Positioner>
      </BaseTooltip.Portal>
    </BaseTooltip.Root>
  );
}

export interface TooltipProviderProps {
  children: ReactNode;
}

/** Shares the open delay so moving between triggers opens the next tooltip instantly. */
export function TooltipProvider({ children }: TooltipProviderProps) {
  return <BaseTooltip.Provider delay={TOOLTIP_DELAY_MS}>{children}</BaseTooltip.Provider>;
}
