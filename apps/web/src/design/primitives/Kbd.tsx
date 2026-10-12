import type { ComponentPropsWithRef } from 'react';
import { cx } from '../cx';
import styles from './Kbd.module.css';

export interface KbdProps extends ComponentPropsWithRef<'kbd'> {
  children: string;
}

/** A key or chord hint: mono 11 px on `--control`, `--r-xs`. */
export function Kbd({ className, children, ...rest }: KbdProps) {
  return (
    <kbd {...rest} className={cx(styles.kbd, className)}>
      {children}
    </kbd>
  );
}
