/**
 * Per-kind fact sheets for the inspector, as plain data so they are testable without React.
 * Units are always written (design-system §15); unknown stays "Not reported", never zero.
 */
import type { Aircraft, Earthquake, FeedId, GeoPoint, SatelliteElements } from '../../contracts';
import { formatAge, formatCount } from '../../design';
import { formatDecimal } from '../../lib/coordinates';
import { lookup } from '../../lib/lookup';
import { spaceMgrs, toMgrs } from '../../lib/mgrs';
import { apsides, orbitalPeriodMin } from '../../lib/orbit';
import { formatUtcDateTime, formatUtcTime, parseTime, secondsBetween } from '../../lib/time';

export type FactTone = 'good' | 'warning' | 'alert' | 'neutral';

export interface Fact {
  readonly label: string;
  readonly value: string;
  /** Identifiers and values compared character by character use the mono face. */
  readonly mono?: boolean;
  /** Shown as glyph + word in this status tone (emergencies, PAGER levels). */
  readonly tone?: FactTone;
}

export const NOT_REPORTED = 'Not reported';
const MINUS = '−';

const SATELLITE_GROUP_LABELS: Readonly<Record<string, string>> = {
  stations: 'Space stations',
  visual: 'Brightest',
  weather: 'Weather',
  'gps-ops': 'GPS',
  galileo: 'Galileo',
  science: 'Science',
};

/** "10.5", "10" — at most `digits` decimals, no trailing zeros. */
export function trimmed(value: number, digits: number): string {
  return String(Number(value.toFixed(digits))).replace('-', MINUS);
}

const text = (value: string | undefined) =>
  value === undefined || value.trim() === '' ? NOT_REPORTED : value.trim();

export function satelliteGroupLabel(group: string): string {
  return lookup(SATELLITE_GROUP_LABELS, group, group);
}

export function positionFacts(position: GeoPoint): Fact[] {
  const mgrs = toMgrs(position.lat, position.lon, 5);
  return [
    { label: 'Position', value: formatDecimal(position), mono: true },
    {
      label: 'MGRS',
      value: mgrs ? spaceMgrs(mgrs) : 'Outside the MGRS bands',
      mono: mgrs !== null,
    },
  ];
}

function verticalRate(fpm: number | undefined): string {
  if (fpm === undefined) return NOT_REPORTED;
  if (fpm === 0) return '0 ft/min (level)';
  return `${fpm > 0 ? '+' : MINUS}${formatCount(Math.abs(fpm))} ft/min`;
}

function altitude(aircraft: Aircraft): string {
  if (aircraft.onGround) return 'On ground';
  const feet = aircraft.altitudeBaroFt ?? aircraft.altitudeGeomFt;
  return feet === undefined ? NOT_REPORTED : `${formatCount(feet)} ft`;
}

export function aircraftFacts(aircraft: Aircraft): Fact[] {
  const emergency = aircraft.emergency && aircraft.emergency !== 'none' ? aircraft.emergency : null;
  return [
    { label: 'Callsign', value: text(aircraft.callsign), mono: Boolean(aircraft.callsign) },
    { label: 'ICAO address', value: aircraft.hex, mono: true },
    {
      label: 'Registration',
      value: text(aircraft.registration),
      mono: Boolean(aircraft.registration),
    },
    { label: 'Type', value: text(aircraft.typeCode), mono: Boolean(aircraft.typeCode) },
    { label: 'Altitude', value: altitude(aircraft) },
    {
      label: 'Ground speed',
      value:
        aircraft.groundSpeedKt === undefined
          ? NOT_REPORTED
          : `${formatCount(aircraft.groundSpeedKt)} kn`,
    },
    {
      label: 'Track',
      value: aircraft.trackDeg === undefined ? NOT_REPORTED : `${Math.round(aircraft.trackDeg)}°`,
    },
    { label: 'Vertical rate', value: verticalRate(aircraft.verticalRateFpm) },
    { label: 'Squawk', value: text(aircraft.squawk), mono: Boolean(aircraft.squawk) },
    emergency
      ? { label: 'Emergency', value: `Declared: ${emergency}`, tone: 'alert' }
      : { label: 'Emergency', value: 'None' },
    { label: 'Military', value: aircraft.isMilitary ? 'Yes' : 'No' },
    ...positionFacts(aircraft.position),
    { label: 'Position age', value: formatAge(aircraft.seenPosS) },
  ];
}

const PAGER_TONES: Readonly<Record<string, FactTone>> = {
  green: 'good',
  yellow: 'warning',
  orange: 'warning',
  red: 'alert',
};

function sentenceCase(value: string): string {
  return value.charAt(0).toUpperCase() + value.slice(1);
}

export function earthquakeFacts(quake: Earthquake, nowMs: number): Fact[] {
  const time = parseTime(quake.time);
  const alert = quake.alert?.toLowerCase();
  const status = quake.status?.toLowerCase();
  return [
    {
      label: 'Magnitude',
      value: `${quake.magnitude.toFixed(1)}${quake.magnitudeType ? ` ${quake.magnitudeType}` : ''}`,
    },
    { label: 'Place', value: text(quake.place) },
    { label: 'Depth', value: `${trimmed(quake.depthKm, 1)} km` },
    {
      label: 'Time',
      value:
        time === null
          ? NOT_REPORTED
          : `${formatUtcDateTime(time)} · ${formatAge(secondsBetween(time, nowMs))} ago`,
      mono: time !== null,
    },
    quake.tsunami
      ? { label: 'Tsunami', value: 'Flagged: check official warnings', tone: 'warning' }
      : { label: 'Tsunami', value: 'Not flagged' },
    alert
      ? {
          label: 'PAGER alert',
          value: sentenceCase(alert),
          tone: lookup<FactTone>(PAGER_TONES, alert, 'neutral'),
        }
      : { label: 'PAGER alert', value: 'None issued' },
    {
      label: 'Felt',
      value:
        quake.felt === undefined
          ? NOT_REPORTED
          : `${formatCount(quake.felt)} ${quake.felt === 1 ? 'report' : 'reports'}`,
    },
    {
      label: 'Significance',
      value: quake.significance === undefined ? NOT_REPORTED : formatCount(quake.significance),
    },
    {
      label: 'Review status',
      value:
        status === 'reviewed' ? 'Reviewed' : status === 'automatic' ? 'Automatic' : text(status),
    },
    { label: 'Event ID', value: quake.id, mono: true },
    ...positionFacts(quake.position),
  ];
}

export function satelliteFacts(satellite: SatelliteElements): Fact[] {
  const epoch = parseTime(satellite.epoch);
  const period = orbitalPeriodMin(satellite.meanMotion);
  const heights = apsides(satellite.meanMotion, satellite.eccentricity);
  return [
    { label: 'Name', value: satellite.name },
    { label: 'NORAD ID', value: String(satellite.noradId), mono: true },
    {
      label: 'International designator',
      value: text(satellite.intlDesignator),
      mono: Boolean(satellite.intlDesignator),
    },
    { label: 'Group', value: satelliteGroupLabel(satellite.group) },
    { label: 'Epoch', value: epoch === null ? NOT_REPORTED : formatUtcDateTime(epoch), mono: true },
    { label: 'Inclination', value: `${trimmed(satellite.inclinationDeg, 2)}°` },
    { label: 'Period', value: period === null ? NOT_REPORTED : `${trimmed(period, 1)} min` },
    {
      label: 'Perigee (approx.)',
      value: heights ? `${formatCount(heights.perigeeKm)} km` : NOT_REPORTED,
    },
    {
      label: 'Apogee (approx.)',
      value: heights ? `${formatCount(heights.apogeeKm)} km` : NOT_REPORTED,
    },
    { label: 'Eccentricity', value: satellite.eccentricity.toFixed(4) },
  ];
}

export interface ProvenanceInput {
  readonly sources: readonly string[];
  readonly observedMs: number | null;
  readonly fetchedMs: number | null;
  readonly nowMs: number;
}

/** Source, observed, fetched and age: where a datum came from and how old it is (§1.5). */
export function provenanceFacts({
  sources,
  observedMs,
  fetchedMs,
  nowMs,
}: ProvenanceInput): Fact[] {
  const at = (ms: number | null) => (ms === null ? NOT_REPORTED : formatUtcTime(ms));
  return [
    { label: 'Source', value: sources.length > 0 ? sources.join(', ') : NOT_REPORTED },
    { label: 'Observed', value: at(observedMs), mono: observedMs !== null },
    { label: 'Fetched', value: at(fetchedMs), mono: fetchedMs !== null },
    {
      label: 'Age',
      value: observedMs === null ? 'Unknown' : formatAge(secondsBetween(observedMs, nowMs)),
    },
  ];
}

export type Inspected =
  | { readonly kind: 'aircraft'; readonly item: Aircraft }
  | { readonly kind: 'earthquakes'; readonly item: Earthquake }
  | { readonly kind: 'satellites'; readonly item: SatelliteElements };

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null;

const isPoint = (value: unknown): value is GeoPoint =>
  isRecord(value) && typeof value.lat === 'number' && typeof value.lon === 'number';

/**
 * Narrows the feeds store's selected record to its kind, checking the fields the inspector
 * relies on and that it is the selected entity (by id), not a previous one.
 */
export function inspect(feed: FeedId, id: string, entity: unknown): Inspected | null {
  if (!isRecord(entity)) return null;
  switch (feed) {
    case 'aircraft':
      return entity.hex === id && isPoint(entity.position)
        ? { kind: 'aircraft', item: entity as unknown as Aircraft }
        : null;
    case 'earthquakes':
      return entity.id === id && isPoint(entity.position) && typeof entity.magnitude === 'number'
        ? { kind: 'earthquakes', item: entity as unknown as Earthquake }
        : null;
    case 'satellites':
      return String(entity.noradId) === id && typeof entity.meanMotion === 'number'
        ? { kind: 'satellites', item: entity as unknown as SatelliteElements }
        : null;
  }
}
