import { useEffect, useRef, useState } from 'react';
import type { SatellitePasses as PassResult, SatellitePass } from '../../contracts';
import { Button, Table, type TableColumn } from '../../design';
import { formatDecimal } from '../../lib/coordinates';
import { formatUtcDateTime, parseTime } from '../../lib/time';
import { toApiError } from '../../platform';
import { useGlobe, usePlatform } from '../../runtime';
import styles from './inspector.module.css';

function utc(iso: string): string {
  const time = parseTime(iso);
  return time === null ? 'Unknown' : formatUtcDateTime(time);
}

const COLUMNS: readonly TableColumn<SatellitePass>[] = [
  { id: 'rise', header: 'Rise', mono: true, cell: (pass) => utc(pass.rise) },
  { id: 'peak', header: 'Peak', mono: true, cell: (pass) => utc(pass.peak) },
  { id: 'set', header: 'Set', mono: true, cell: (pass) => utc(pass.set) },
  {
    id: 'elevation',
    header: 'Max elevation',
    align: 'end',
    cell: (pass) => `${pass.maxElevationDeg.toFixed(1)}°`,
  },
  {
    id: 'start',
    header: 'Window start',
    cell: (pass) => (pass.alreadyAbove ? 'Ongoing at window start' : 'Rises in window'),
  },
  {
    id: 'end',
    header: 'Window end',
    cell: (pass) => (pass.continuesAfterWindow ? 'Clipped at window end' : 'Sets in window'),
  },
];

/** Explicit observer predictions never start following or moving the selected satellite. */
export function SatellitePasses({ noradId }: { readonly noradId: number }) {
  const platform = usePlatform();
  const globe = useGlobe();
  const [result, setResult] = useState<PassResult | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const request = useRef<AbortController | null>(null);

  useEffect(() => {
    setResult((previous) => (previous?.noradId === noradId ? previous : null));
    setError(null);
    setPending(false);
    return () => {
      request.current?.abort();
      request.current = null;
    };
  }, [noradId]);

  const predict = async () => {
    if (pending) return;
    const centre = globe?.centrePosition();
    if (
      !centre ||
      !Number.isFinite(centre.lat) ||
      !Number.isFinite(centre.lon) ||
      Math.abs(centre.lat) > 90 ||
      Math.abs(centre.lon) > 180
    ) {
      setError('Point the map centre at the ground before choosing an observer.');
      return;
    }
    request.current?.abort();
    const next = new AbortController();
    request.current = next;
    setPending(true);
    setError(null);
    setResult(null);
    try {
      const passes = await platform.satellitePasses(
        {
          noradId,
          observer: { lat: centre.lat, lon: centre.lon, altM: 0 },
          hours: 24,
          minElevationDeg: 0,
        },
        { signal: next.signal },
      );
      if (!next.signal.aborted && request.current === next) setResult(passes);
    } catch (cause) {
      if (!next.signal.aborted && request.current === next) setError(toApiError(cause).message);
    } finally {
      if (!next.signal.aborted && request.current === next) setPending(false);
    }
  };

  return (
    <section aria-label="Satellite pass predictions">
      <h3 className={styles.subhead}>Predicted passes</h3>
      <Button size="sm" disabled={pending || globe === null} onClick={() => void predict()}>
        {pending ? 'Predicting passes…' : 'Predict passes at map centre'}
      </Button>
      <p className={styles.historyNote}>
        Geometric passes at or above {result?.minElevationDeg ?? 0}° elevation. Sunlight,
        atmospheric refraction and naked-eye visibility are not modelled.
      </p>
      <p className={styles.historyNote} role="status">
        {error ??
          (pending
            ? 'Calculating the next 24 hours…'
            : result
              ? `${result.passes.length} predicted passes`
              : 'Choose a map position, then predict the next 24 hours at ground level.')}
      </p>
      {result ? (
        <>
          <p className={styles.historyNote}>
            Observer: {formatDecimal(result.observer)} · {result.observer.altM ?? 0} m altitude
          </p>
          <p className={styles.historyNote}>
            Window: {utc(result.from)} – {utc(result.until)}
          </p>
          {result.passes.length === 0 ? (
            <p className={styles.historyNote}>
              No geometric passes reach this elevation in the requested window.
            </p>
          ) : (
            <Table
              aria-label="Satellite passes"
              columns={COLUMNS}
              rows={result.passes}
              getRowId={(pass) => `${pass.rise}:${pass.set}`}
              maxHeight="200px"
              minWidth="1080px"
            />
          )}
        </>
      ) : null}
    </section>
  );
}
