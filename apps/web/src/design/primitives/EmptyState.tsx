import type { ReactNode } from 'react';
import { cx } from '../cx';
import type { Icon } from '../icons';
import styles from './States.module.css';

export interface EmptyStateProps {
  icon: Icon;
  /** Optional short heading. */
  title?: string;
  /** One sentence saying what to do. */
  children: ReactNode;
  /** One action. */
  action?: ReactNode;
  className?: string | undefined;
}

/** Glyph (Phosphor fill, 24 px, `--ink-3`), one sentence, one action. */
export function EmptyState({ icon: Glyph, title, children, action, className }: EmptyStateProps) {
  return (
    <div className={cx(styles.state, className)} data-kind="empty">
      <Glyph size={24} weight="fill" aria-hidden className={styles.glyph} />
      {title ? <p className={styles.title}>{title}</p> : null}
      <p className={styles.body}>{children}</p>
      {action ? <div className={styles.action}>{action}</div> : null}
    </div>
  );
}
