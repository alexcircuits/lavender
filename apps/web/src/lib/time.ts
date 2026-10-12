/**
 * Time helpers. The chrome shows UTC (design-system §15); wire times are ISO 8601 strings that
 * are untrusted input, so parsing returns null rather than NaN.
 */

/** Milliseconds since the epoch, or null for a missing or unparseable time. */
export function parseTime(iso: string | null | undefined): number | null {
  if (typeof iso !== 'string' || iso === '') return null;
  const ms = Date.parse(iso);
  return Number.isFinite(ms) ? ms : null;
}

/** Whole seconds from `fromMs` to `toMs`, never negative (clock skew reads as "now"). */
export function secondsBetween(fromMs: number, toMs: number): number {
  return Math.max(0, Math.floor((toMs - fromMs) / 1000));
}

/** `14:02:11 UTC` */
export function formatUtcTime(ms: number): string {
  return `${new Date(ms).toISOString().slice(11, 19)} UTC`;
}

/** `2026-10-11 14:02:11 UTC` */
export function formatUtcDateTime(ms: number): string {
  const iso = new Date(ms).toISOString();
  return `${iso.slice(0, 10)} ${iso.slice(11, 19)} UTC`;
}
