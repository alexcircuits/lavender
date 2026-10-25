/**
 * Per-kind fact sheets for the inspector, as plain data so they are testable without React.
 * Units are always written (design-system §15); unknown stays "Not reported", never zero.
 */
import type {
  Aircraft,
  BikeshareStation,
  Cyclone,
  Earthquake,
  FeedId,
  FireCell,
  GeoPoint,
  Launch,
  SatelliteElements,
  TransitVehicle,
  Vessel,
  VesselDimensions,
  VesselEta,
  VesselVoyage,
} from '../../contracts';
import { formatAge, formatCount } from '../../design';
import { fireCellId, isValidFireCell } from '../../feeds/fireCells';
import { formatDecimal } from '../../lib/coordinates';
import { isLaunchRecord, type LaunchPadGroup } from '../../lib/launches';
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
  'last-30-days': 'Recent launches',
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

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
const isWhole = (value: unknown, min: number, max: number): value is number =>
  typeof value === 'number' && Number.isInteger(value) && value >= min && value <= max;
const twoDigits = (value: number) => String(value).padStart(2, '0');

/**
 * An AIS ETA as the crew entered it: "17 Oct 08:30 UTC", or "17 Oct" without a time. AIS
 * carries no year, so none is shown; impossible values read as not reported.
 */
export function formatVesselEta(eta: VesselEta | undefined): string {
  if (typeof eta !== 'object' || eta === null) return NOT_REPORTED;
  if (!isWhole(eta.month, 1, 12) || !isWhole(eta.day, 1, 31)) return NOT_REPORTED;
  const date = `${eta.day} ${MONTHS[eta.month - 1]}`;
  if (!isWhole(eta.hour, 0, 23)) return date;
  const minute = isWhole(eta.minute, 0, 59) ? eta.minute : 0;
  return `${date} ${twoDigits(eta.hour)}:${twoDigits(minute)} UTC`;
}

/** Length and beam: "70 × 12 m", "70 m long", "12 m beam". */
export function formatVesselDimensions(dimensions: VesselDimensions | undefined): string {
  const metres = (value: number | undefined) =>
    isWhole(value, 1, 1_022) ? formatCount(value) : null;
  const length = metres(dimensions?.lengthM);
  const beam = metres(dimensions?.beamM);
  if (length && beam) return `${length} × ${beam} m`;
  if (length) return `${length} m long`;
  if (beam) return `${beam} m beam`;
  return NOT_REPORTED;
}

/** AIS static draught: 0 means "not available"; it cannot exceed 25.5 m. */
const draught = (metres: number | undefined) =>
  typeof metres === 'number' && Number.isFinite(metres) && metres > 0 && metres <= 25.5
    ? `${trimmed(metres, 1)} m`
    : NOT_REPORTED;

/** Text from the wire, or "Not reported" when it is missing, blank or not text at all. */
const wireText = (value: unknown) =>
  typeof value === 'string' && value.trim() !== '' ? value.trim() : NOT_REPORTED;

/**
 * The selected ship's voyage and status (call sign, navigational status, ETA, draught,
 * dimensions). They come with its track response, not in feed snapshots (`VesselVoyage`).
 */
export function vesselVoyageFacts(voyage: VesselVoyage | undefined): Fact[] {
  const callSign = wireText(voyage?.callSign);
  return [
    { label: 'Call sign', value: callSign, mono: callSign !== NOT_REPORTED },
    { label: 'Navigational status', value: wireText(voyage?.navigationalStatus) },
    { label: 'ETA', value: formatVesselEta(voyage?.eta) },
    { label: 'Draught', value: draught(voyage?.draughtM) },
    { label: 'Length × beam', value: formatVesselDimensions(voyage?.dimensions) },
  ];
}

/** AIS "not available" codes stay unknown, even for records from a newer server. */
export function vesselFacts(vessel: Vessel, nowMs: number): Fact[] {
  const reported = (value: number | undefined, max: number, unit: string) =>
    value !== undefined && Number.isFinite(value) && value >= 0 && value < max
      ? `${trimmed(value, 1)}${unit}`
      : NOT_REPORTED;
  const observed = parseTime(vessel.lastPositionUtc);
  return [
    { label: 'Name', value: text(vessel.name) },
    { label: 'MMSI', value: vessel.mmsi, mono: true },
    { label: 'IMO', value: text(vessel.imo), mono: Boolean(vessel.imo) },
    { label: 'Vessel type', value: text(vessel.vesselType) },
    { label: 'Destination', value: text(vessel.destination) },
    { label: 'Speed over ground', value: reported(vessel.speedKnots, 102.3, ' kn') },
    { label: 'Course over ground', value: reported(vessel.courseDeg, 360, '°') },
    {
      label: 'Heading',
      value: vessel.headingDeg === 360 ? '360°' : reported(vessel.headingDeg, 360, '°'),
    },
    {
      label: 'Position time',
      value: observed === null ? NOT_REPORTED : formatUtcDateTime(observed),
      mono: observed !== null,
    },
    {
      label: 'Position age',
      value: observed === null ? NOT_REPORTED : formatAge(secondsBetween(observed, nowMs)),
    },
    ...positionFacts(vessel.position),
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
  /** The snapshot's notices, e.g. that fixture data is replayed, not live. */
  readonly notices?: readonly string[];
}

export function cycloneFacts(storm: Cyclone): Fact[] {
  const at = (value: string) => {
    const time = parseTime(value);
    return time === null ? NOT_REPORTED : formatUtcDateTime(time);
  };
  const basin = lookup(
    { AL: 'Atlantic', EP: 'Eastern Pacific', CP: 'Central Pacific' },
    storm.basin.toUpperCase(),
    storm.basin,
  );
  return [
    { label: 'Storm ID', value: storm.id, mono: true },
    { label: 'Classification', value: storm.classification },
    { label: 'Basin', value: basin },
    { label: 'Advisory', value: storm.advisoryNumber, mono: true },
    { label: 'Issued', value: at(storm.issuedAt), mono: true },
    { label: 'Position time', value: at(storm.positionAt), mono: true },
    {
      label: 'Maximum sustained wind',
      value: storm.intensityKt === undefined ? NOT_REPORTED : `${storm.intensityKt} kn`,
    },
    {
      label: 'Central pressure',
      value: storm.pressureMb === undefined ? NOT_REPORTED : `${storm.pressureMb} mb`,
    },
    {
      label: 'Movement direction',
      value:
        storm.movement.directionDegrees === undefined
          ? NOT_REPORTED
          : `${storm.movement.directionDegrees}°`,
    },
    {
      label: 'Movement speed',
      value: storm.movement.speedKt === undefined ? NOT_REPORTED : `${storm.movement.speedKt} kn`,
    },
    {
      label: 'Forecast geometry',
      value:
        storm.geometryStatus === 'current'
          ? `Current · advisory ${storm.geometryAdvisoryNumber ?? storm.advisoryNumber}`
          : storm.geometryStatus === 'pending'
            ? 'Pending'
            : 'Unavailable',
      tone: storm.geometryStatus === 'current' ? 'neutral' : 'warning',
    },
    ...positionFacts(storm.position),
  ];
}

/**
 * Source, observed, fetched and age: where a datum came from and how old it is (§1.5), plus
 * the snapshot's notices when it has any (fixture replay, partial coverage, truncation).
 */
export function provenanceFacts({
  sources,
  observedMs,
  fetchedMs,
  nowMs,
  notices = [],
}: ProvenanceInput): Fact[] {
  const at = (ms: number | null) => (ms === null ? NOT_REPORTED : formatUtcTime(ms));
  const said = notices.map((notice) => notice.trim()).filter((notice) => notice !== '');
  return [
    { label: 'Source', value: sources.length > 0 ? sources.join(', ') : NOT_REPORTED },
    { label: 'Observed', value: at(observedMs), mono: observedMs !== null },
    { label: 'Fetched', value: at(fetchedMs), mono: fetchedMs !== null },
    {
      label: 'Age',
      value: observedMs === null ? 'Unknown' : formatAge(secondsBetween(observedMs, nowMs)),
    },
    ...(said.length > 0 ? [{ label: 'Notices', value: said.join(' ') }] : []),
  ];
}

/** Brightness temperature in kelvin; the server sends 0 when FIRMS did not report it. */
function brightnessK(kelvin: number): string {
  return Number.isFinite(kelvin) && kelvin > 0 ? `${kelvin.toFixed(1)} K` : NOT_REPORTED;
}

export function fireFacts(cell: FireCell): Fact[] {
  const isCluster = cell.count > 1;
  const brightness = brightnessK(cell.brightness);
  const facts: Fact[] = [
    { label: 'Radiative power', value: `${cell.frp.toFixed(1)} MW`, mono: true },
  ];
  if (isCluster) {
    facts.push(
      { label: 'Detections', value: formatCount(cell.count) },
      { label: 'Peak FRP', value: `${cell.maxFrp.toFixed(1)} MW`, mono: true },
    );
  }
  facts.push(
    { label: 'Brightness', value: brightness, mono: brightness !== NOT_REPORTED },
    { label: 'Confidence', value: text(cell.confidence) },
    { label: 'Satellite', value: text(cell.satellite) },
    {
      label: 'Acquisition',
      value: cell.dayNight === 'D' ? 'Day' : cell.dayNight === 'N' ? 'Night' : text(cell.dayNight),
    },
    ...positionFacts({ lat: cell.lat, lon: cell.lon }),
  );
  return facts;
}

const stringOr = (value: unknown): string => (typeof value === 'string' ? value : '');

/**
 * A fire cell the inspector can describe: exactly the cells the globe draws (the shared
 * `isValidFireCell` rule) and only the selected one (by id). Returns a normalised copy in
 * which malformed descriptive fields read as unknown (`NaN` brightness, empty strings), so a
 * drawn symbol never leads to a crash or a missing sheet.
 */
function fireRecord(entity: Record<string, unknown>, id: string): FireCell | null {
  if (!isValidFireCell(entity) || fireCellId(entity) !== id) return null;
  const { lat, lon, count, frp, maxFrp } = entity;
  return {
    lat,
    lon,
    count,
    frp,
    maxFrp,
    brightness: typeof entity.brightness === 'number' ? entity.brightness : Number.NaN,
    confidence: stringOr(entity.confidence),
    satellite: stringOr(entity.satellite),
    acquiredAt: stringOr(entity.acquiredAt),
    dayNight: stringOr(entity.dayNight),
    latestAcquiredAt: stringOr(entity.latestAcquiredAt),
  };
}

export type Inspected =
  | { readonly kind: 'aircraft'; readonly item: Aircraft }
  | { readonly kind: 'earthquakes'; readonly item: Earthquake }
  | { readonly kind: 'satellites'; readonly item: SatelliteElements }
  | { readonly kind: 'cyclones'; readonly item: Cyclone }
  | { readonly kind: 'fires'; readonly item: FireCell }
  | { readonly kind: 'vessels'; readonly item: Vessel }
  | { readonly kind: 'launches'; readonly item: Launch }
  | { readonly kind: 'bikeshare'; readonly item: BikeshareStation }
  | { readonly kind: 'transit'; readonly item: TransitVehicle }
  /** A launch pad and its launches (selected on the globe; see `useLaunchPadView`). */
  | { readonly kind: 'launchPad'; readonly item: LaunchPadGroup };

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
    case 'cyclones':
      return entity.id === id &&
        isPoint(entity.position) &&
        typeof entity.name === 'string' &&
        isRecord(entity.movement)
        ? { kind: 'cyclones', item: entity as unknown as Cyclone }
        : null;
    case 'fires': {
      const cell = fireRecord(entity, id);
      return cell ? { kind: 'fires', item: cell } : null;
    }
    case 'launches':
      return isLaunchRecord(entity, id) ? { kind: 'launches', item: entity } : null;
    case 'perimeters':
      return null;
    case 'vessels':
      return entity.mmsi === id &&
        isPoint(entity.position) &&
        Number.isFinite(entity.position.lat) &&
        Number.isFinite(entity.position.lon) &&
        Math.abs(entity.position.lat) <= 90 &&
        Math.abs(entity.position.lon) <= 180 &&
        ['name', 'imo', 'vesselType', 'destination', 'lastPositionUtc'].every(
          (key) => entity[key] === undefined || typeof entity[key] === 'string',
        )
        ? { kind: 'vessels', item: entity as unknown as Vessel }
        : null;
    case 'bikeshare':
      return entity.id === id && isPoint(entity.position)
        ? { kind: 'bikeshare', item: entity as unknown as BikeshareStation }
        : null;
    case 'transit':
      return entity.id === id && isPoint(entity.position)
        ? { kind: 'transit', item: entity as unknown as TransitVehicle }
        : null;
    default:
      return null;
  }
}
