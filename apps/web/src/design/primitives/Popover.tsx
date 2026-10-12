import { Popover as BasePopover } from '@base-ui/react/popover';
import type { ReactNode } from 'react';
import { cx } from '../cx';
import styles from './Popover.module.css';
import { usePortalProps } from './portal';

/**
 * Popover (Base UI): same surface as menus, no arrow, 8 px offset; closes on Esc and outside
 * click; focus returns to the trigger.
 *
 *   <Popover>
 *     <PopoverTrigger render={<Button>Feeds</Button>} />
 *     <PopoverContent title="Feed health">…</PopoverContent>
 *   </Popover>
 */
export const Popover = BasePopover.Root;
export const PopoverTrigger = BasePopover.Trigger;
export const PopoverClose = BasePopover.Close;

export interface PopoverContentProps {
  /** Visible title; also names the popup for assistive technology. */
  title?: string;
  description?: ReactNode;
  children?: ReactNode;
  side?: 'top' | 'bottom' | 'left' | 'right';
  align?: 'start' | 'center' | 'end';
  className?: string | undefined;
}

export function PopoverContent({
  title,
  description,
  children,
  side = 'bottom',
  align = 'start',
  className,
}: PopoverContentProps) {
  const portal = usePortalProps();
  return (
    <BasePopover.Portal {...portal}>
      <BasePopover.Positioner
        side={side}
        align={align}
        sideOffset={8}
        collisionPadding={8}
        className={styles.positioner}
      >
        <BasePopover.Popup className={cx(styles.popup, className)}>
          {title ? <BasePopover.Title className={styles.title}>{title}</BasePopover.Title> : null}
          {description ? (
            <BasePopover.Description className={styles.description}>
              {description}
            </BasePopover.Description>
          ) : null}
          {children}
        </BasePopover.Popup>
      </BasePopover.Positioner>
    </BasePopover.Portal>
  );
}
