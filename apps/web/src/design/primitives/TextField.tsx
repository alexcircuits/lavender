import { type ComponentPropsWithRef, type ReactNode, useId } from 'react';
import { cx } from '../cx';
import styles from './TextField.module.css';

export interface TextFieldProps extends Omit<ComponentPropsWithRef<'input'>, 'size'> {
  /** Visible label (sentence case). Use `hideLabel` only where context names the field. */
  label: string;
  hideLabel?: boolean;
  /** Help text under the field. */
  description?: ReactNode;
  /** What is wrong and what to do. Shown in --warning (errors are amber, red is for alerts). */
  error?: string;
  /** Content before the input, e.g. an icon. */
  leading?: ReactNode;
  /** Content after the input, e.g. a unit or a clear button. */
  trailing?: ReactNode;
  size?: 'sm' | 'md';
  /** Use the mono face for values compared character by character (keys, coordinates). */
  mono?: boolean;
}

/** Single-line field: `--well` fill, no border; hover adds an inset hairline; focus ring. */
export function TextField({
  label,
  hideLabel = false,
  description,
  error,
  leading,
  trailing,
  size = 'md',
  mono = false,
  id,
  className,
  disabled,
  ...rest
}: TextFieldProps) {
  const autoId = useId();
  const inputId = id ?? autoId;
  const descriptionId = description ? `${inputId}-description` : undefined;
  const errorId = error ? `${inputId}-error` : undefined;
  const describedBy = [errorId, descriptionId].filter(Boolean).join(' ') || undefined;

  return (
    <div className={cx(styles.field, className)} data-disabled={disabled ? '' : undefined}>
      <label htmlFor={inputId} className={hideLabel ? 'visually-hidden' : styles.label}>
        {label}
      </label>
      <div
        className={styles.control}
        data-size={size}
        data-invalid={error ? '' : undefined}
        data-disabled={disabled ? '' : undefined}
      >
        {leading ? <span className={styles.leading}>{leading}</span> : null}
        <input
          {...rest}
          id={inputId}
          disabled={disabled}
          aria-invalid={error ? true : undefined}
          aria-describedby={describedBy}
          className={cx(styles.input, mono && 'mono')}
        />
        {trailing ? <span className={styles.trailing}>{trailing}</span> : null}
      </div>
      {error ? (
        <p id={errorId} className={styles.error}>
          {error}
        </p>
      ) : null}
      {description ? (
        <p id={descriptionId} className={styles.description}>
          {description}
        </p>
      ) : null}
    </div>
  );
}
