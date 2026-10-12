import type { ComponentPropsWithRef, ReactNode } from 'react';
import { cx } from '../cx';
import type { Icon } from '../icons';
import styles from './Button.module.css';

/**
 * `primary` — accent fill, at most one per surface. `secondary` — tonal (`--control`).
 * `ghost` — transparent, hover `--island-hover`. `danger` — `--alert` text on `--control`.
 * `danger-fill` — solid alert fill, for the confirm button of a destructive dialog only.
 */
export type ButtonVariant = 'primary' | 'secondary' | 'ghost' | 'danger' | 'danger-fill';
/** sm 28 (compact rows, toolbars) · md 32 (default) · lg 36 (prominent) · xl 40 (dialog primary). */
export type ButtonSize = 'sm' | 'md' | 'lg' | 'xl';

export interface ButtonProps extends ComponentPropsWithRef<'button'> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  /** Leading icon, drawn in Phosphor's bold weight (actions). */
  icon?: Icon;
  /** Trailing icon, e.g. a caret on a menu trigger. */
  iconEnd?: Icon;
  children: ReactNode;
}

export function Button({
  variant = 'secondary',
  size = 'md',
  icon: LeadingIcon,
  iconEnd: TrailingIcon,
  className,
  children,
  type = 'button',
  disabled,
  ...rest
}: ButtonProps) {
  const iconSize = size === 'sm' ? 14 : 16;
  return (
    <button
      {...rest}
      type={type}
      disabled={disabled}
      data-variant={variant}
      data-size={size}
      data-disabled={disabled ? '' : undefined}
      className={cx(styles.button, className)}
    >
      {LeadingIcon ? (
        <LeadingIcon className={styles.icon} size={iconSize} weight="bold" aria-hidden />
      ) : null}
      <span className={styles.label}>{children}</span>
      {TrailingIcon ? (
        <TrailingIcon className={styles.icon} size={iconSize} weight="bold" aria-hidden />
      ) : null}
    </button>
  );
}
