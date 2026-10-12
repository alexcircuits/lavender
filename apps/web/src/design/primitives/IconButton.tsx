import type { ComponentPropsWithRef } from 'react';
import { cx } from '../cx';
import type { Icon } from '../icons';
import styles from './IconButton.module.css';
import { Tooltip } from './Tooltip';

export type IconButtonVariant = 'ghost' | 'secondary' | 'primary';
export type IconButtonSize = 'sm' | 'md' | 'lg';

export interface IconButtonProps
  extends Omit<ComponentPropsWithRef<'button'>, 'children' | 'aria-label'> {
  /** Required: the accessible name, also shown as the tooltip. Sentence case, a plain verb. */
  label: string;
  icon: Icon;
  variant?: IconButtonVariant;
  size?: IconButtonSize;
  /** Key hint shown in the tooltip, e.g. "+" or "⌘\\". */
  shortcut?: string;
  /** For toggle buttons: exposes aria-pressed and the pressed style. */
  pressed?: boolean;
  tooltipSide?: 'top' | 'bottom' | 'left' | 'right';
}

/** Square, icon-only button with a required label and a tooltip after 500 ms hover or on focus. */
export function IconButton({
  label,
  icon: Glyph,
  variant = 'ghost',
  size = 'md',
  shortcut,
  pressed,
  tooltipSide = 'top',
  className,
  type = 'button',
  disabled,
  ...rest
}: IconButtonProps) {
  const iconSize = size === 'sm' ? 14 : 16;
  const button = (
    <button
      {...rest}
      type={type}
      disabled={disabled}
      aria-label={label}
      aria-pressed={pressed}
      data-variant={variant}
      data-size={size}
      data-pressed={pressed ? '' : undefined}
      data-disabled={disabled ? '' : undefined}
      className={cx(styles.iconButton, className)}
    >
      <Glyph size={iconSize} weight={pressed ? 'fill' : 'bold'} aria-hidden />
    </button>
  );
  return (
    <Tooltip content={label} side={tooltipSide} {...(shortcut ? { shortcut } : {})}>
      {button}
    </Tooltip>
  );
}
