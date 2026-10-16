import { launchPhase } from '../../lib/launches';
import { launchStatusSpec } from './format';
import styles from './launches.module.css';

export interface LaunchStatusChipProps {
  /** The status as Launch Library 2 reports it, e.g. "Go for Launch". */
  readonly status: string;
}

/** A launch status as icon + word in its status tone (never hue alone, design-system §3.3). */
export function LaunchStatusChip({ status }: LaunchStatusChipProps) {
  const spec = launchStatusSpec(launchPhase(status), status);
  const Glyph = spec.icon;
  return (
    <span className={styles.status} data-tone={spec.tone} data-launch-status="">
      <Glyph size={10} weight="fill" aria-hidden className={styles.statusGlyph} />
      <span className={styles.statusWord}>{spec.word}</span>
    </span>
  );
}
