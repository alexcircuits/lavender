/**
 * Fact sheets for a launch and for a launch pad, as plain data (testable without React).
 * Times are written in UTC with a relative phrase on the same clock the list uses; unknown
 * stays "Not reported", never guessed.
 */
import type { Launch } from '../../contracts';
import { formatCount } from '../../design';
import {
  awaitingOutcome,
  isPlaceablePad,
  type LaunchPadGroup,
  launchPhase,
  launchTitle,
  splitLaunches,
} from '../../lib/launches';
import { formatUtcDateTime, parseTime } from '../../lib/time';
import {
  formatRelative,
  formatUtcMinute,
  formatWindow,
  launchStatusSpec,
} from '../launches/format';
import { type Fact, NOT_REPORTED, positionFacts } from './facts';

const text = (value: string | undefined) => value?.trim() || NOT_REPORTED;

/** "2026-06-02 18:00:00 UTC · in 6 h". */
function timeFact(label: string, iso: string | undefined, nowMs: number): Fact {
  const ms = parseTime(iso);
  return ms === null
    ? { label, value: NOT_REPORTED }
    : { label, value: `${formatUtcDateTime(ms)} · ${formatRelative(ms, nowMs)}`, mono: true };
}

function windowFact(launch: Pick<Launch, 'windowStart' | 'windowEnd'>): Fact {
  const start = parseTime(launch.windowStart);
  const end = parseTime(launch.windowEnd);
  let value = NOT_REPORTED;
  if (start !== null && end !== null) value = formatWindow(start, end);
  else if (start !== null) value = `Opens ${formatUtcMinute(start)}`;
  else if (end !== null) value = `Closes ${formatUtcMinute(end)}`;
  return { label: 'Window', value, mono: value !== NOT_REPORTED };
}

/**
 * The status in its tone. A passed NET with a pre-launch status says the outcome is missing,
 * as a warning: the source has not caught up, whatever the status word says.
 */
export function launchStatusFact(launch: Launch, nowMs: number): Fact {
  const spec = launchStatusSpec(launchPhase(launch.status), launch.status);
  return awaitingOutcome(launch, nowMs)
    ? { label: 'Status', value: `${spec.word} · no outcome reported yet`, tone: 'warning' }
    : { label: 'Status', value: spec.word, tone: spec.tone };
}

export function launchFacts(launch: Launch, nowMs: number): Fact[] {
  const pad = isPlaceablePad(launch.pad) ? launch.pad : null;
  return [
    launchStatusFact(launch, nowMs),
    timeFact('No earlier than (NET)', launch.net, nowMs),
    windowFact(launch),
    { label: 'Provider', value: text(launch.provider) },
    { label: 'Rocket', value: text(launch.rocket) },
    { label: 'Mission', value: text(launch.mission) },
    { label: 'Orbit', value: text(launch.orbit) },
    { label: 'Launch name', value: text(launch.name) },
    {
      label: 'Launch designator',
      value: text(launch.launchDesignator),
      mono: Boolean(launch.launchDesignator),
    },
    { label: 'Pad', value: text(launch.pad?.name) },
    { label: 'Location', value: text(launch.pad?.locationName) },
    ...(pad ? positionFacts({ lat: pad.latitude, lon: pad.longitude }) : []),
    { label: 'Launch ID', value: launch.id, mono: true },
  ];
}

/** "Mission A · in 6 h"; a launch without a NET says so. */
function nextOrLast(launch: Launch | undefined, nowMs: number): string {
  if (!launch) return 'None reported';
  const net = parseTime(launch.net);
  const when = net === null ? 'NET not reported' : formatRelative(net, nowMs);
  return `${launchTitle(launch)} · ${when}`;
}

export function padFacts(pad: LaunchPadGroup, nowMs: number): Fact[] {
  const { upcoming, recent } = splitLaunches(pad.launches, nowMs);
  return [
    { label: 'Pad', value: pad.name },
    { label: 'Location', value: text(pad.locationName) },
    {
      label: 'Launches',
      value: `${formatCount(pad.launches.length)} · ${formatCount(upcoming.length)} upcoming, ${formatCount(recent.length)} recent`,
    },
    { label: 'Next launch', value: nextOrLast(upcoming[0], nowMs) },
    { label: 'Latest launch', value: nextOrLast(recent[0], nowMs) },
    ...positionFacts({ lat: pad.latitude, lon: pad.longitude }),
  ];
}
