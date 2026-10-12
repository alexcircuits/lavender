import { Dialog as BaseDialog } from '@base-ui/react/dialog';
import type { ReactElement, ReactNode } from 'react';
import { cx } from '../cx';
import { XIcon } from '../icons';
import styles from './Dialog.module.css';
import { IconButton } from './IconButton';
import { usePortalProps } from './portal';

export interface DialogProps {
  /** The element that opens the dialog; focus returns to it on close. */
  trigger?: ReactElement;
  open?: boolean;
  defaultOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  /** Sentence case question or statement: "Remove the FIRMS key?" */
  title: string;
  description?: ReactNode;
  children?: ReactNode;
  /** Footer actions, primary last. Wrap the dismissing button in <DialogClose render={…} />. */
  actions?: ReactNode;
  /**
   * Destructive confirmations do not close on Esc or an outside click, and show no close
   * glyph: the person must choose an action (design-system §7).
   */
  destructive?: boolean;
  className?: string | undefined;
}

export const DialogClose = BaseDialog.Close;

/** Modal dialog (Base UI): `--r-xl`, scrim without blur, focus trapped and returned. */
export function Dialog({
  trigger,
  open,
  defaultOpen,
  onOpenChange,
  title,
  description,
  children,
  actions,
  destructive = false,
  className,
}: DialogProps) {
  const portal = usePortalProps();
  return (
    <BaseDialog.Root
      {...(open === undefined ? {} : { open })}
      {...(defaultOpen === undefined ? {} : { defaultOpen })}
      disablePointerDismissal={destructive}
      onOpenChange={(next, details) => {
        if (!next && destructive && details.reason === 'escape-key') {
          details.cancel();
          return;
        }
        onOpenChange?.(next);
      }}
    >
      {trigger ? <BaseDialog.Trigger render={trigger} /> : null}
      <BaseDialog.Portal {...portal}>
        <BaseDialog.Backdrop className={styles.backdrop} />
        <BaseDialog.Popup
          className={cx(styles.popup, className)}
          data-destructive={destructive ? '' : undefined}
        >
          <BaseDialog.Title className={styles.title}>{title}</BaseDialog.Title>
          {description ? (
            <BaseDialog.Description className={styles.description}>
              {description}
            </BaseDialog.Description>
          ) : null}
          {children ? <div className={styles.body}>{children}</div> : null}
          {actions ? <footer className={styles.actions}>{actions}</footer> : null}
          {/* Last in DOM order so initial focus lands on the first field or action, not on
              "Close"; it still sits top-right. Destructive dialogs have none. */}
          {destructive ? null : (
            <BaseDialog.Close
              className={styles.close}
              render={<IconButton label="Close" icon={XIcon} size="sm" tooltipSide="bottom" />}
            />
          )}
        </BaseDialog.Popup>
      </BaseDialog.Portal>
    </BaseDialog.Root>
  );
}
