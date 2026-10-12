/**
 * Display formatting used by design primitives. Units are always shown (design-system §15).
 * Grouping uses a narrow no-break space, as the spec writes counts ("1 248").
 */
const GROUP_SEPARATOR = ' ';

/** "1 248" — integer counts with a narrow no-break space between thousands. */
export function formatCount(value: number): string {
  if (!Number.isFinite(value)) return '—';
  return Math.round(value).toLocaleString('en-US').replaceAll(',', GROUP_SEPARATOR);
}

/** Compact age: "now", "42 s", "4 min", "2 h 5 min", "3 d". Unknown stays unknown. */
export function formatAge(seconds: number | null | undefined): string {
  if (seconds === null || seconds === undefined || !Number.isFinite(seconds) || seconds < 0) {
    return 'unknown';
  }
  const s = Math.floor(seconds);
  if (s < 1) return 'now';
  if (s < 60) return `${s} s`;
  const minutes = Math.floor(s / 60);
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  if (hours < 48) {
    const rest = minutes % 60;
    return rest === 0 ? `${hours} h` : `${hours} h ${rest} min`;
  }
  return `${Math.floor(hours / 24)} d`;
}

/** "every 1 min", "every 15 s" — a cadence written for people. */
export function formatCadence(seconds: number): string {
  return `every ${formatAge(seconds)}`;
}

/** Exact age for tooltips: "42 s", "1 min 35 s", "2 h 5 min". */
export function formatAgeExact(seconds: number | null | undefined): string {
  if (seconds === null || seconds === undefined || !Number.isFinite(seconds) || seconds < 0) {
    return 'unknown';
  }
  const s = Math.floor(seconds);
  if (s < 60) return `${s} s`;
  if (s < 3600) {
    const rest = s % 60;
    const minutes = Math.floor(s / 60);
    return rest === 0 ? `${minutes} min` : `${minutes} min ${rest} s`;
  }
  return formatAge(s);
}
