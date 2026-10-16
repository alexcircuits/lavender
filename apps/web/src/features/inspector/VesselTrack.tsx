import { useEffect, useMemo, useState } from 'react';
import type { ApiError, Vessel, VesselTrackResponse, VesselTrackSample } from '../../contracts';
import { Button, formatAge, Table, type TableColumn } from '../../design';
import { formatDecimal } from '../../lib/coordinates';
import { formatUtcTime, parseTime } from '../../lib/time';
import { toApiError } from '../../platform';
import { useGlobe, usePlatform, useSystem } from '../../runtime';
import { FactSheet } from './FactSheet';
import { vesselVoyageFacts } from './facts';
import styles from './inspector.module.css';

const COLUMNS: readonly TableColumn<VesselTrackSample>[] = [
  {
    id: 'time',
    header: 'Observed',
    mono: true,
    cell: (sample) => formatUtcTime(sample.epochSec * 1000),
  },
  {
    id: 'position',
    header: 'Position',
    mono: true,
    cell: (sample) => formatDecimal({ lat: sample.lat, lon: sample.lon }),
  },
];

interface TrackState {
  readonly pending: boolean;
  /** The last track that loaded; kept when a refresh fails (last good data stays). */
  readonly track: VesselTrackResponse | null;
  readonly error: ApiError | null;
}

/** Rows listed at most (the renderer draws at most as many samples; the server keeps 64). */
const MAX_TRACK_ROWS = 1_000;
/**
 * A newer position report reloads the track at most once per this many seconds of report
 * time: the server adds a track point at most every 30 s (vessel_store `TRACK_MIN_GAP_S`).
 */
const REFRESH_EVERY_S = 30;

const isUsable = (sample: VesselTrackSample) =>
  typeof sample === 'object' &&
  sample !== null &&
  Number.isFinite(sample.lat) &&
  Number.isFinite(sample.lon) &&
  Math.abs(sample.lat) <= 90 &&
  Math.abs(sample.lon) <= 180 &&
  Number.isFinite(sample.epochSec);

const sampleKey = (sample: VesselTrackSample) => `${sample.epochSec}:${sample.lat},${sample.lon}`;

export interface TrackSummary {
  /** Distinct usable samples, newest first, at most `MAX_TRACK_ROWS`. */
  readonly rows: readonly VesselTrackSample[];
  /** Distinct usable samples in the whole track. */
  readonly count: number;
  /** Distinct places among them: the globe draws a trail only from two places on. */
  readonly places: number;
  /** Seconds from the oldest to the newest sample. */
  readonly spanSec: number;
}

/** The track's usable, distinct samples (repeated ones once), newest first, and its span. */
export function summariseTrack(track: VesselTrackResponse): TrackSummary {
  if (!Array.isArray(track.samples)) return { rows: [], count: 0, places: 0, spanSec: 0 };
  const seen = new Set<string>();
  const places = new Set<string>();
  const distinct: VesselTrackSample[] = [];
  let oldest = Number.POSITIVE_INFINITY;
  let newest = Number.NEGATIVE_INFINITY;
  for (const sample of track.samples) {
    if (!isUsable(sample)) continue;
    const key = sampleKey(sample);
    if (seen.has(key)) continue;
    seen.add(key);
    places.add(`${sample.lat},${sample.lon}`);
    distinct.push(sample);
    oldest = Math.min(oldest, sample.epochSec);
    newest = Math.max(newest, sample.epochSec);
  }
  distinct.sort((a, b) => b.epochSec - a.epochSec);
  return {
    rows: distinct.slice(0, MAX_TRACK_ROWS),
    count: distinct.length,
    places: places.size,
    spanSec: distinct.length > 1 ? newest - oldest : 0,
  };
}

/** Where the positions came from and whether the globe can draw them as a trail. */
function trailNote(summary: TrackSummary | null): string {
  if (summary !== null && summary.places > 1) return 'drawn on the globe as the selection trail.';
  if (summary !== null && summary.count > 1) return 'all from one place, so no trail is drawn.';
  return 'a trail is drawn once the ship reports from another place.';
}

function describe(summary: TrackSummary, source: string): string {
  const count = `${summary.count} ${summary.count === 1 ? 'position' : 'positions'}`;
  const listed =
    summary.rows.length < summary.count ? `, newest ${summary.rows.length} listed` : '';
  const span = summary.spanSec > 0 ? ` over ${formatAge(summary.spanSec)}` : '';
  return [`${count}${span}${listed}`, source].join(' · ');
}

function failureMessage(error: ApiError): string {
  return error.code === 'notFound'
    ? 'This server has kept no positions for this vessel yet.'
    : error.message;
}

/** When a ship last reported its position, in epoch seconds, if it says. */
export function vesselReportedAtSec(
  vessel: Pick<Vessel, 'lastPositionEpoch' | 'lastPositionUtc'>,
): number | undefined {
  if (typeof vessel.lastPositionEpoch === 'number' && Number.isFinite(vessel.lastPositionEpoch)) {
    return vessel.lastPositionEpoch;
  }
  const ms = parseTime(vessel.lastPositionUtc);
  return ms === null ? undefined : ms / 1000;
}

export interface VesselTrackProps {
  readonly mmsi: string;
  /** When the ship's latest position was reported (epoch seconds), if known. */
  readonly reportedAtSec?: number | undefined;
}

/**
 * The selected vessel's voyage details (call sign, status, ETA, draught, dimensions; they
 * come with the track, not in feed snapshots) and recent track: the positions this backend
 * kept from the live AIS stream (bounded per ship and by its retention window, never
 * fetched upstream), loaded through the platform when the sheet opens and again when the
 * ship reports a newer position (at most every `REFRESH_EVERY_S` of report time), listed
 * newest first and drawn on the globe as the selection trail once there are two places to
 * join. The trail is removed when the sheet closes or shows another vessel.
 */
export function VesselTrack({ mmsi, reportedAtSec }: VesselTrackProps) {
  const platform = usePlatform();
  const globe = useGlobe();
  const catalog = useSystem((s) => s.catalog);
  const [generation, setGeneration] = useState(0);
  const [state, setState] = useState<TrackState>({ pending: true, track: null, error: null });
  const reportWindow =
    typeof reportedAtSec === 'number' && Number.isFinite(reportedAtSec)
      ? Math.floor(reportedAtSec / REFRESH_EVERY_S)
      : null;

  // biome-ignore lint/correctness/useExhaustiveDependencies: `generation` (the refresh button) and `reportWindow` (a newer report) request a reload.
  useEffect(() => {
    const request = new AbortController();
    setState((previous) => ({ ...previous, pending: true }));
    platform.vesselTrack(mmsi, { signal: request.signal }).then(
      (track) => {
        if (!request.signal.aborted) setState({ pending: false, track, error: null });
      },
      (cause: unknown) => {
        if (request.signal.aborted) return;
        setState((previous) => ({ ...previous, pending: false, error: toApiError(cause) }));
      },
    );
    return () => request.abort();
  }, [platform, mmsi, generation, reportWindow]);

  const { track, pending, error } = state;
  useEffect(() => {
    if (!globe) return;
    globe.showVesselTrack(track);
    return () => globe.showVesselTrack(null);
  }, [globe, track]);

  const summary = useMemo(() => (track ? summariseTrack(track) : null), [track]);
  const source = track
    ? (catalog?.sources.find((s) => s.id === track.source)?.name ?? track.source)
    : '';

  let note: string;
  if (error && track) note = `The track could not be refreshed. ${failureMessage(error)}`;
  else if (error) note = failureMessage(error);
  else if (summary) note = describe(summary, source);
  else note = 'Loading recent track…';

  return (
    <>
      {track ? (
        <section aria-label="Voyage">
          <h3 className={styles.subhead}>Voyage</h3>
          <FactSheet facts={vesselVoyageFacts(track.voyage)} label="Voyage" />
        </section>
      ) : null}
      <section aria-label="Recent vessel track">
        <h3 className={styles.subhead}>Recent track</h3>
        <Button size="sm" disabled={pending} onClick={() => setGeneration((g) => g + 1)}>
          {pending && track ? 'Refreshing track…' : 'Refresh track'}
        </Button>
        <p className={styles.historyNote} role="status">
          {note}
        </p>
        {track ? (
          <p className={styles.historyNote}>
            Positions this server kept from the AIS stream, for at most{' '}
            {formatAge(track.retainedSec)}; {trailNote(summary)}
          </p>
        ) : null}
        {summary && summary.rows.length > 0 ? (
          <Table
            aria-label="Vessel track"
            columns={COLUMNS}
            rows={summary.rows}
            getRowId={sampleKey}
            maxHeight="200px"
            minWidth="320px"
          />
        ) : null}
      </section>
    </>
  );
}
