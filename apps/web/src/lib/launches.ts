/**
 * Rocket launches as plain data: where a launch stands (from Launch Library 2's status), which
 * launches are upcoming or recent on a given clock, and the launch pads that aggregate them.
 * Engine-free and React-free, so the globe's pad markers, the launch list and the inspector
 * agree on every rule, and each rule is testable with a fixed clock.
 */
import type { Launch, LaunchPad } from '../contracts';
import { lookup } from './lookup';
import { parseTime } from './time';

/** Where a launch stands, read from its Launch Library 2 status. */
export type LaunchPhase =
  | 'go'
  | 'tbc'
  | 'tbd'
  | 'hold'
  | 'inFlight'
  | 'success'
  | 'deployed'
  | 'partialFailure'
  | 'failure'
  | 'unknown';

/** LL2 status names, and the abbreviations older answers (and our fixtures) use. */
const PHASES: Readonly<Record<string, LaunchPhase>> = {
  'go for launch': 'go',
  go: 'go',
  'to be confirmed': 'tbc',
  tbc: 'tbc',
  'to be determined': 'tbd',
  tbd: 'tbd',
  'on hold': 'hold',
  hold: 'hold',
  'launch in flight': 'inFlight',
  'in flight': 'inFlight',
  'launch successful': 'success',
  success: 'success',
  'payload deployed': 'deployed',
  deployed: 'deployed',
  'launch was a partial failure': 'partialFailure',
  'partial failure': 'partialFailure',
  'launch failure': 'failure',
  failure: 'failure',
};

export function launchPhase(status: string): LaunchPhase {
  return lookup(PHASES, status.trim().toLowerCase(), 'unknown');
}

/** Whether the source reports that the launch has lifted off (in flight or an outcome). */
export function hasLiftedOff(phase: LaunchPhase): boolean {
  return (
    phase === 'inFlight' ||
    phase === 'success' ||
    phase === 'deployed' ||
    phase === 'partialFailure' ||
    phase === 'failure'
  );
}

/**
 * The name to show: the mission, else the part of LL2's "Rocket | Mission" name after the bar,
 * else the whole name.
 */
export function launchTitle(launch: Pick<Launch, 'name' | 'mission'>): string {
  const mission = launch.mission?.trim();
  if (mission) return mission;
  const bar = launch.name.lastIndexOf('|');
  const afterBar = bar >= 0 ? launch.name.slice(bar + 1).trim() : '';
  return afterBar || launch.name.trim() || 'Unnamed launch';
}

const isOptionalText = (value: unknown) => value === undefined || typeof value === 'string';

/** A pad's names are text; its position is checked where it is placed (`isPlaceablePad`). */
function isPadRecord(value: unknown): boolean {
  if (typeof value !== 'object' || value === null) return false;
  const p = value as Partial<Record<keyof LaunchPad, unknown>>;
  return typeof p.name === 'string' && isOptionalText(p.locationName);
}

/**
 * Whether a record from the wire is the launch `id` with the fields the interface reads: text
 * where text is expected, payload names as strings, matched satellites as catalogue numbers.
 * A pad that cannot be placed is still a launch (shown without a position).
 */
export function isLaunchRecord(value: unknown, id: string): value is Launch {
  if (typeof value !== 'object' || value === null) return false;
  const l = value as Partial<Record<keyof Launch, unknown>>;
  return (
    l.id === id &&
    typeof l.name === 'string' &&
    typeof l.status === 'string' &&
    (['net', 'windowStart', 'windowEnd', 'launchDesignator', 'provider', 'rocket'] as const).every(
      (key) => isOptionalText(l[key]),
    ) &&
    (['mission', 'missionDescription', 'orbit'] as const).every((key) => isOptionalText(l[key])) &&
    (l.pad === undefined || isPadRecord(l.pad)) &&
    Array.isArray(l.payloadNames) &&
    l.payloadNames.every((name) => typeof name === 'string') &&
    Array.isArray(l.matchedNoradIds) &&
    l.matchedNoradIds.every((norad) => Number.isInteger(norad) && norad > 0)
  );
}

/** Whether a record from the wire is a launch (with a non-empty id) the interface can show. */
export function isWellFormedLaunch(value: unknown): value is Launch {
  if (typeof value !== 'object' || value === null) return false;
  const { id } = value as { readonly id?: unknown };
  return typeof id === 'string' && id !== '' && isLaunchRecord(value, id);
}

/**
 * The launches a snapshot carries that the interface can show. A malformed record is skipped
 * (the list, the pads and the inspector all read through this), never fatal to the others.
 */
export function wellFormedLaunches(items: readonly unknown[]): Launch[] {
  return items.filter(isWellFormedLaunch);
}

// ── The launch clock ─────────────────────────────────────────────────────────────────────

/** Launch times are written to the minute; the list and the inspector read this clock. */
export const LAUNCH_CLOCK_STEP_MS = 60_000;

/**
 * `nowMs` floored to the minute. Every launch surface groups and phrases times on this one
 * clock, so the list and the sheet beside it never disagree about whether a NET has passed.
 */
export function launchClock(nowMs: number): number {
  return Math.floor(nowMs / LAUNCH_CLOCK_STEP_MS) * LAUNCH_CLOCK_STEP_MS;
}

// ── Upcoming and recent ──────────────────────────────────────────────────────────────────

export interface LaunchTimeline {
  /** NET still ahead (or not set yet and no lift-off reported), soonest first. */
  readonly upcoming: readonly Launch[];
  /** NET passed (or lift-off reported without a NET), latest first. */
  readonly recent: readonly Launch[];
}

const byName = (a: Launch, b: Launch) => launchTitle(a).localeCompare(launchTitle(b));

/** Ascending times with unknown times last; ties by name. */
function soonestFirst(a: Launch, b: Launch): number {
  const at = parseTime(a.net);
  const bt = parseTime(b.net);
  if (at !== bt) {
    if (at === null) return 1;
    if (bt === null) return -1;
    return at - bt;
  }
  return byName(a, b);
}

/** Descending times with unknown times last; ties by name. */
function latestFirst(a: Launch, b: Launch): number {
  const at = parseTime(a.net);
  const bt = parseTime(b.net);
  if (at !== bt) {
    if (at === null) return 1;
    if (bt === null) return -1;
    return bt - at;
  }
  return byName(a, b);
}

/** Whether `launch` is still ahead at `nowMs`: its NET (no earlier than) is in the future. */
export function isUpcoming(launch: Launch, nowMs: number): boolean {
  const net = parseTime(launch.net);
  if (net === null) return !hasLiftedOff(launchPhase(launch.status));
  return net > nowMs;
}

/**
 * Splits launches into upcoming and recent by their NET on the given clock, the way LL2's own
 * upcoming and previous lists do. The status is shown beside the time, never used to move a
 * launch between the groups, so a stale status stays visible instead of being hidden.
 */
export function splitLaunches(launches: readonly Launch[], nowMs: number): LaunchTimeline {
  const upcoming: Launch[] = [];
  const recent: Launch[] = [];
  for (const launch of launches) (isUpcoming(launch, nowMs) ? upcoming : recent).push(launch);
  return { upcoming: upcoming.sort(soonestFirst), recent: recent.sort(latestFirst) };
}

/**
 * A launch whose NET has passed while the source still reports a pre-launch status (go, to be
 * confirmed, on hold…): its outcome has not been reported yet. Unknown statuses say nothing.
 */
export function awaitingOutcome(launch: Launch, nowMs: number): boolean {
  const phase = launchPhase(launch.status);
  return !isUpcoming(launch, nowMs) && phase !== 'unknown' && !hasLiftedOff(phase);
}

// ── Launch pads ──────────────────────────────────────────────────────────────────────────

/**
 * Pad selections are `pad:<lat>,<lon>`. Launch ids are LL2's UUIDs (numeric ids or slugs from
 * older answers), which do not start with this prefix.
 */
export const PAD_ID_PREFIX = 'pad:';

export function isPadId(id: string): boolean {
  return id.startsWith(PAD_ID_PREFIX);
}

const inRange = (value: unknown, limit: number): value is number =>
  typeof value === 'number' && Number.isFinite(value) && Math.abs(value) <= limit;

/** Whether a pad from the wire can be placed: finite latitude within ±90°, longitude ±180°. */
export function isPlaceablePad(pad: unknown): pad is LaunchPad {
  if (typeof pad !== 'object' || pad === null) return false;
  const p = pad as Partial<Record<keyof LaunchPad, unknown>>;
  return inRange(p.latitude, 90) && inRange(p.longitude, 180) && typeof p.name === 'string';
}

/** A coordinate to 4 decimals; a value that rounds to zero is "0.0000" on both sides of it. */
const padCoordinate = (degrees: number) => degrees.toFixed(4).replace(/^-(0\.0+)$/, '$1');

/** One marker's identity: pads at the same place (to about 11 m) are one site on the map. */
export function padIdOf(pad: Pick<LaunchPad, 'latitude' | 'longitude'>): string {
  return `${PAD_ID_PREFIX}${padCoordinate(pad.latitude)},${padCoordinate(pad.longitude)}`;
}

export interface LaunchPadGroup {
  readonly id: string;
  /** The pad's name; distinct names at one place are joined. */
  readonly name: string;
  readonly locationName: string | undefined;
  readonly latitude: number;
  readonly longitude: number;
  /** In snapshot order; use `splitLaunches` to order them on a clock. */
  readonly launches: readonly Launch[];
}

interface Building {
  readonly id: string;
  readonly names: string[];
  locationName: string | undefined;
  readonly latitude: number;
  readonly longitude: number;
  readonly launches: Launch[];
}

/**
 * Launches aggregated by pad, in the order each pad first appears. Launches without a pad that
 * can be placed are left out (the list still shows them).
 */
export function groupLaunchPads(launches: readonly Launch[]): LaunchPadGroup[] {
  const pads = new Map<string, Building>();
  for (const launch of launches) {
    const pad = launch.pad;
    if (!isPlaceablePad(pad)) continue;
    const id = padIdOf(pad);
    let group = pads.get(id);
    if (!group) {
      group = {
        id,
        names: [],
        locationName: undefined,
        latitude: pad.latitude,
        longitude: pad.longitude,
        launches: [],
      };
      pads.set(id, group);
    }
    const name = pad.name.trim();
    if (name && !group.names.includes(name)) group.names.push(name);
    group.locationName ??= pad.locationName?.trim() || undefined;
    group.launches.push(launch);
  }
  return [...pads.values()].map(({ names, ...group }) => ({
    ...group,
    name: names.join(' / ') || 'Unnamed launch pad',
  }));
}

/** What picking a marker selects: its launch when the pad has one, else the pad. */
export function padPickId(group: LaunchPadGroup): string {
  const [only, ...others] = group.launches;
  return only && others.length === 0 ? only.id : group.id;
}

/** The pad group a selection refers to: a pad id directly, or the pad of a launch. */
export function findPadGroup(
  groups: readonly LaunchPadGroup[],
  id: string,
): LaunchPadGroup | undefined {
  return isPadId(id)
    ? groups.find((group) => group.id === id)
    : groups.find((group) => group.launches.some((launch) => launch.id === id));
}
