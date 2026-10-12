import { cx } from '../cx';
import { formatAgeExact, formatCadence } from '../format';
import styles from './FreshnessMeter.module.css';
import { Tooltip } from './Tooltip';

/** The bar is full at this many cadences: 3× is where symbols start drawing as stale (§11). */
export const FRESHNESS_SCALE = 3;
/** Past this many cadences the meter turns from good to warning (§7). */
export const FRESHNESS_WARNING_AT = 2;

export type FreshnessState = 'fresh' | 'late' | 'unknown';

export interface FreshnessMeterProps {
  /** Seconds since the source last delivered data; `null` when unknown (never shown as 0). */
  ageSeconds: number | null;
  /** Expected seconds between updates. */
  cadenceSeconds: number;
  /** Layer name for the accessible label, e.g. "Aircraft". */
  label: string;
  className?: string | undefined;
}

export function freshnessState(ageSeconds: number | null, cadenceSeconds: number): FreshnessState {
  if (ageSeconds === null || !Number.isFinite(ageSeconds) || cadenceSeconds <= 0) return 'unknown';
  return ageSeconds > cadenceSeconds * FRESHNESS_WARNING_AT ? 'late' : 'fresh';
}

/** "42 s old, updates every 1 min" — the exact age in words, for the tooltip and valuetext. */
export function describeFreshness(ageSeconds: number | null, cadenceSeconds: number): string {
  if (ageSeconds === null || !Number.isFinite(ageSeconds)) {
    return `Age unknown, updates ${formatCadence(cadenceSeconds)}`;
  }
  return `${formatAgeExact(ageSeconds)} old, updates ${formatCadence(cadenceSeconds)}`;
}

/**
 * Lavender's signature data element: a 24 × 4 px bar in layer rows. Fill = age ÷ cadence on a
 * 0–3× scale; good while within 2× the cadence, warning past it. role="meter" with the age as
 * the value; the tooltip states the exact age.
 */
export function FreshnessMeter({
  ageSeconds,
  cadenceSeconds,
  label,
  className,
}: FreshnessMeterProps) {
  const state = freshnessState(ageSeconds, cadenceSeconds);
  const text = describeFreshness(ageSeconds, cadenceSeconds);
  const max = cadenceSeconds * FRESHNESS_SCALE;
  const fraction = state === 'unknown' || ageSeconds === null ? 0 : Math.min(1, ageSeconds / max);

  const meter =
    state === 'unknown' ? (
      <span
        role="img"
        aria-label={`${label} freshness: ${text}`}
        data-state={state}
        className={cx(styles.meter, className)}
      />
    ) : (
      // biome-ignore lint/a11y/useSemanticElements: a 24 × 4 px bar must render identically in WebKit, Blink and Gecko and trigger a tooltip; native <meter> styling differs per engine (pseudo-elements).
      <span
        role="meter"
        aria-label={`${label} freshness`}
        aria-valuemin={0}
        aria-valuemax={Math.round(max)}
        aria-valuenow={Math.round(ageSeconds ?? 0)}
        aria-valuetext={text}
        data-state={state}
        className={cx(styles.meter, className)}
      >
        <span className={styles.fill} style={{ transform: `scaleX(${fraction})` }} />
      </span>
    );

  return <Tooltip content={text}>{meter}</Tooltip>;
}
