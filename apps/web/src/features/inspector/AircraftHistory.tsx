import { useEffect, useRef, useState } from 'react';
import type { AircraftTrack, TrackPoint } from '../../contracts';
import { Button, Table, type TableColumn } from '../../design';
import { formatDecimal } from '../../lib/coordinates';
import { formatUtcTime, parseTime } from '../../lib/time';
import { toApiError } from '../../platform';
import { useGlobe, usePlatform, useSystem } from '../../runtime';
import styles from './inspector.module.css';

const COLUMNS: readonly TableColumn<TrackPoint>[] = [
  {
    id: 'time',
    header: 'Observed',
    mono: true,
    cell: (point) => {
      const time = parseTime(point.time);
      return time === null ? 'Unknown' : formatUtcTime(time);
    },
  },
  {
    id: 'position',
    header: 'Position',
    mono: true,
    cell: (point) => formatDecimal(point.position),
  },
];

/** On-demand history is bounded on the server and never initiates an upstream fetch. */
export function AircraftHistory({ hex }: { readonly hex: string }) {
  const platform = usePlatform();
  const globe = useGlobe();
  const catalog = useSystem((state) => state.catalog);
  const [track, setTrack] = useState<AircraftTrack | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const request = useRef<AbortController | null>(null);
  useEffect(() => () => request.current?.abort(), []);

  // biome-ignore lint/correctness/useExhaustiveDependencies: hex change resets history state and track.
  useEffect(() => {
    setTrack(null);
    setError(null);
  }, [hex]);

  useEffect(() => {
    if (!globe) return;
    globe.showAircraftTrack(track);
    return () => globe.showAircraftTrack(null);
  }, [globe, track]);

  const load = async () => {
    request.current?.abort();
    const next = new AbortController();
    request.current = next;
    setPending(true);
    setError(null);
    try {
      const result = await platform.aircraftTrack(hex, { signal: next.signal });
      if (!next.signal.aborted) setTrack(result);
    } catch (cause) {
      if (!next.signal.aborted) setError(toApiError(cause).message);
    } finally {
      if (!next.signal.aborted) setPending(false);
    }
  };
  const sources = track?.sources
    .map((id) => catalog?.sources.find((source) => source.id === id)?.name ?? id)
    .join(' · ');
  return (
    <section aria-label="Recent aircraft history">
      <h3 className={styles.subhead}>Recent history</h3>
      <Button size="sm" disabled={pending} onClick={() => void load()}>
        {pending ? 'Loading history…' : track ? 'Refresh history' : 'Load recent history'}
      </Button>
      <p className={styles.historyNote} role="status">
        {error ??
          (track
            ? `${track.points.length} retained observations${sources ? ` · ${sources}` : ''}`
            : 'Positions observed by this server during the current session.')}
      </p>
      {track && track.points.length > 0 ? (
        <Table
          aria-label="Aircraft history"
          columns={COLUMNS}
          rows={track.points}
          getRowId={(point) => point.time}
          maxHeight="200px"
          minWidth="320px"
        />
      ) : null}
    </section>
  );
}
