/**
 * How launches are written in the interface: a launch status as icon + word in a status tone
 * (never hue alone, design-system §3.3), and launch times as a relative phrase beside an
 * absolute UTC time (§15).
 */
import {
  CheckCircleIcon,
  CheckIcon,
  formatAge,
  type Icon,
  PauseIcon,
  QuestionIcon,
  RocketLaunchIcon,
  WarningCircleIcon,
  XCircleIcon,
} from '../../design';
import type { LaunchPhase } from '../../lib/launches';

export type LaunchTone = 'good' | 'warning' | 'alert' | 'neutral';

export interface LaunchStatusSpec {
  readonly word: string;
  readonly tone: LaunchTone;
  readonly icon: Icon;
}

export const LAUNCH_STATUS: Readonly<Record<LaunchPhase, LaunchStatusSpec>> = {
  go: { word: 'Go', tone: 'good', icon: CheckIcon },
  tbc: { word: 'To be confirmed', tone: 'neutral', icon: QuestionIcon },
  tbd: { word: 'To be determined', tone: 'neutral', icon: QuestionIcon },
  hold: { word: 'On hold', tone: 'warning', icon: PauseIcon },
  inFlight: { word: 'In flight', tone: 'good', icon: RocketLaunchIcon },
  success: { word: 'Success', tone: 'good', icon: CheckCircleIcon },
  deployed: { word: 'Payload deployed', tone: 'good', icon: CheckCircleIcon },
  partialFailure: { word: 'Partial failure', tone: 'warning', icon: WarningCircleIcon },
  failure: { word: 'Failure', tone: 'alert', icon: XCircleIcon },
  unknown: { word: 'Unknown', tone: 'neutral', icon: QuestionIcon },
};

/** The status as Lavender writes it; a status this client does not know keeps its own words. */
export function launchStatusSpec(phase: LaunchPhase, status: string): LaunchStatusSpec {
  const spec = LAUNCH_STATUS[phase];
  const raw = status.trim();
  return phase === 'unknown' && raw !== '' ? { ...spec, word: raw } : spec;
}

/** Within a minute either way a launch time reads as "now". */
const NOW_WINDOW_S = 60;

/** "in 6 h", "24 h ago", "in 63 d", "now" — `targetMs` relative to `nowMs`. */
export function formatRelative(targetMs: number, nowMs: number): string {
  const seconds = Math.round((targetMs - nowMs) / 1000);
  if (Math.abs(seconds) < NOW_WINDOW_S) return 'now';
  const age = formatAge(Math.abs(seconds));
  return seconds > 0 ? `in ${age}` : `${age} ago`;
}

const iso = (ms: number) => new Date(ms).toISOString();

/** "2026-06-02 18:00 UTC": launch times to the minute, as the list writes them. */
export function formatUtcMinute(ms: number): string {
  const t = iso(ms);
  return `${t.slice(0, 10)} ${t.slice(11, 16)} UTC`;
}

/**
 * A launch window: "2026-06-02 17:45–21:45 UTC (4 h)", across midnight with both dates, and
 * "2026-08-04 15:02 UTC (instantaneous)" when it opens and closes at once.
 */
export function formatWindow(startMs: number, endMs: number): string {
  if (endMs < startMs) return `${formatUtcMinute(startMs)} – ${formatUtcMinute(endMs)}`;
  if (endMs === startMs) return `${formatUtcMinute(startMs)} (instantaneous)`;
  const start = iso(startMs);
  const end = iso(endMs);
  const length = formatAge((endMs - startMs) / 1000);
  const sameDay = start.slice(0, 10) === end.slice(0, 10);
  const close = sameDay ? end.slice(11, 16) : `${end.slice(0, 10)} ${end.slice(11, 16)}`;
  return `${start.slice(0, 10)} ${start.slice(11, 16)}–${close} UTC (${length})`;
}
