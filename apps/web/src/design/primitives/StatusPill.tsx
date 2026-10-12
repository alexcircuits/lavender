import type { ComponentPropsWithRef } from 'react';
import { cx } from '../cx';
import {
  CircleHalfIcon,
  CircleIcon,
  type Icon,
  type IconWeight,
  LockSimpleIcon,
  SquareIcon,
  TriangleIcon,
} from '../icons';
import styles from './StatusPill.module.css';

/** Feed and source states (design-system §7, §11). Status is never colour alone. */
export type FeedStatus =
  | 'live'
  | 'stale'
  | 'degraded'
  | 'unavailable'
  | 'not-configured'
  | 'licence';

type Tone = 'good' | 'warning' | 'alert' | 'neutral';

interface StatusSpec {
  word: string;
  icon: Icon;
  weight: IconWeight;
  tone: Tone;
}

/** ● Live · ◐ Stale · ▲ Degraded · ■ Unavailable · ○ Not configured · Licence. */
export const STATUS_SPECS: Readonly<Record<FeedStatus, StatusSpec>> = Object.freeze({
  live: { word: 'Live', icon: CircleIcon, weight: 'fill', tone: 'good' },
  stale: { word: 'Stale', icon: CircleHalfIcon, weight: 'fill', tone: 'warning' },
  degraded: { word: 'Degraded', icon: TriangleIcon, weight: 'fill', tone: 'warning' },
  unavailable: { word: 'Unavailable', icon: SquareIcon, weight: 'fill', tone: 'alert' },
  'not-configured': { word: 'Not configured', icon: CircleIcon, weight: 'bold', tone: 'neutral' },
  licence: { word: 'Licence', icon: LockSimpleIcon, weight: 'fill', tone: 'neutral' },
});

export interface StatusPillProps extends Omit<ComponentPropsWithRef<'span'>, 'children'> {
  status: FeedStatus;
  /** `sm` (16 px) fits the second line of a dense row; `md` (20 px) stands alone. */
  size?: 'sm' | 'md';
  /** Qualifier after the word, e.g. "4 min" → "Stale 4 min". */
  detail?: string;
}

/** Icon + word, coloured by status token. The live dot pulses three times, then holds. */
export function StatusPill({ status, detail, size = 'md', className, ...rest }: StatusPillProps) {
  const spec = STATUS_SPECS[status];
  const Glyph = spec.icon;
  return (
    <span
      {...rest}
      data-status={status}
      data-tone={spec.tone}
      data-size={size}
      className={cx(styles.pill, className)}
    >
      <Glyph size={10} weight={spec.weight} aria-hidden className={styles.glyph} />
      <span className={styles.word}>
        {spec.word}
        {detail ? <span className={styles.detail}> {detail}</span> : null}
      </span>
    </span>
  );
}

export type BadgeTone = 'neutral' | 'accent';

export interface BadgeProps extends ComponentPropsWithRef<'span'> {
  tone?: BadgeTone;
}

/** Small count or label chip in `--type-micro` (e.g. "12 new"). */
export function Badge({ tone = 'neutral', className, ...rest }: BadgeProps) {
  return <span {...rest} data-tone={tone} className={cx(styles.badge, className)} />;
}
