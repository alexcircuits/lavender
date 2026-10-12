import type { ReactNode } from 'react';
import { cx } from '../cx';
import { WarningCircleIcon, WarningIcon } from '../icons';
import styles from './States.module.css';

export interface ErrorStateProps {
  /** `alert` when action is needed (source unavailable); `warning` when it may recover. */
  severity?: 'alert' | 'warning';
  /** What happened, plainly: "Earthquakes are unavailable". */
  title: string;
  /** What to do, and what Lavender is doing meanwhile. */
  children: ReactNode;
  /** Retry or Configure. */
  action?: ReactNode;
  /** Announce when it appears (use for errors that replace content the person was reading). */
  live?: boolean;
  className?: string | undefined;
}

/** What happened + what to do + an action; `--alert` or `--warning` glyph per severity. */
export function ErrorState({
  severity = 'alert',
  title,
  children,
  action,
  live = false,
  className,
}: ErrorStateProps) {
  const Glyph = severity === 'alert' ? WarningCircleIcon : WarningIcon;
  return (
    <div
      className={cx(styles.state, className)}
      data-kind="error"
      data-severity={severity}
      {...(live ? { role: 'alert' } : {})}
    >
      <Glyph size={24} weight="fill" aria-hidden className={styles.glyph} />
      <p className={styles.title}>{title}</p>
      <p className={styles.body}>{children}</p>
      {action ? <div className={styles.action}>{action}</div> : null}
    </div>
  );
}
