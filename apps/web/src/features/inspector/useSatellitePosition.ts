import { useEffect, useState } from 'react';
import type { GeoPoint } from '../../contracts';
import { parseTime } from '../../lib/time';
import { usePlatform } from '../../runtime';

/** A satellite moves about 7.5 km/s; the point is re-asked this often while shown. */
export const SATELLITE_REFRESH_MS = 30_000;

export interface SatellitePosition {
  readonly status: 'loading' | 'ready' | 'unavailable';
  readonly position: GeoPoint | null;
  /** When the sub-satellite point was computed (ms), shown next to it. */
  readonly atMs: number | null;
}

const LOADING: SatellitePosition = { status: 'loading', position: null, atMs: null };
const UNAVAILABLE: SatellitePosition = { status: 'unavailable', position: null, atMs: null };

/**
 * The satellite's current sub-satellite point, asked of the core (which propagates with SGP4)
 * as a one-minute track and refreshed every 30 s. Mean elements alone carry no position, and
 * Fly to and Copy coordinates need one. A failed refresh keeps the last point, whose time is
 * shown, rather than pretending to know where the satellite is now.
 */
export function useSatellitePosition(noradId: number | null): SatellitePosition {
  const platform = usePlatform();
  const [state, setState] = useState<SatellitePosition>(noradId === null ? UNAVAILABLE : LOADING);

  useEffect(() => {
    if (noradId === null) return;
    let request: AbortController | null = null;
    const refresh = () => {
      request?.abort();
      const current = new AbortController();
      request = current;
      platform
        .satelliteTrack({ noradId, minutes: 1, stepS: 60 }, { signal: current.signal })
        .then((track) => {
          if (current.signal.aborted) return;
          const first = track.points[0];
          setState((previous) =>
            first
              ? { status: 'ready', position: first.position, atMs: parseTime(first.time) }
              : previous.position
                ? previous
                : UNAVAILABLE,
          );
        })
        .catch(() => {
          if (!current.signal.aborted) {
            setState((previous) => (previous.position ? previous : UNAVAILABLE));
          }
        });
    };
    refresh();
    const timer = setInterval(refresh, SATELLITE_REFRESH_MS);
    return () => {
      clearInterval(timer);
      request?.abort();
    };
  }, [platform, noradId]);

  return state;
}
