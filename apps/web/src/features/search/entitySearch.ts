/**
 * Live-entity search over the snapshots the feed controller already holds, so callsigns, ICAO
 * addresses and NORAD ids resolve instantly and still work when the server's search does not.
 */
import type { Aircraft, EntityHit, FeedId, FeedSnapshot } from '../../contracts';
import { formatCount } from '../../design';

export interface SnapshotSource {
  latest(feed: FeedId): FeedSnapshot | undefined;
}

interface Scored {
  readonly hit: EntityHit;
  readonly score: number;
}

/** 0 exact, 1 prefix, 2 contains, null no match (case-insensitive). */
function score(value: string | undefined, q: string): number | null {
  if (!value) return null;
  const v = value.trim().toLowerCase();
  if (v === q) return 0;
  if (v.startsWith(q)) return 1;
  return v.includes(q) ? 2 : null;
}

function best(values: ReadonlyArray<string | undefined>, q: string): number | null {
  const scores = values.map((v) => score(v, q)).filter((s): s is number => s !== null);
  return scores.length > 0 ? Math.min(...scores) : null;
}

function aircraftDetail(a: Aircraft): string {
  const altitude = a.onGround
    ? 'on ground'
    : a.altitudeBaroFt === undefined
      ? null
      : `${formatCount(a.altitudeBaroFt)} ft`;
  return [a.typeCode, a.registration, altitude].filter(Boolean).join(' · ');
}

function scoreSnapshot(snapshot: FeedSnapshot, q: string): Scored[] {
  const { items } = snapshot;
  switch (items.kind) {
    case 'aircraft':
      return items.items.flatMap((a) => {
        const s = best([a.callsign, a.hex, a.registration], q);
        if (s === null) return [];
        const detail = aircraftDetail(a);
        return [
          {
            score: s,
            hit: {
              feed: 'aircraft' as const,
              id: a.hex,
              label: a.callsign?.trim() || a.hex,
              ...(detail ? { detail } : {}),
              position: a.position,
            },
          },
        ];
      });
    case 'satellites':
      return items.items.flatMap((sat) => {
        const s = best([sat.name, String(sat.noradId), sat.intlDesignator], q);
        if (s === null) return [];
        return [
          {
            score: s,
            hit: {
              feed: 'satellites' as const,
              id: String(sat.noradId),
              label: sat.name,
              detail: `NORAD ${sat.noradId}`,
            },
          },
        ];
      });
    case 'earthquakes':
      return items.items.flatMap((quake) => {
        // Place names are prose; only match them on a meaningful query.
        const s = best([quake.id, q.length >= 3 ? quake.place : undefined], q);
        if (s === null) return [];
        return [
          {
            score: s,
            hit: {
              feed: 'earthquakes' as const,
              id: quake.id,
              label: `M ${quake.magnitude.toFixed(1)} · ${quake.place ?? quake.id}`,
              detail: quake.id,
              position: quake.position,
            },
          },
        ];
      });
    case 'cyclones':
      return items.items.flatMap((storm) => {
        const s = best([storm.id, storm.name], q);
        return s === null
          ? []
          : [
              {
                score: s,
                hit: {
                  feed: 'cyclones' as const,
                  id: storm.id,
                  label: storm.name,
                  detail: `${storm.classification} · ${storm.basin}`,
                  position: storm.position,
                },
              },
            ];
      });
  }
}

const ALL_FEEDS: readonly FeedId[] = ['aircraft', 'satellites', 'earthquakes', 'cyclones'];

/** The best `limit` matches across `feeds` (default: all), best match first. */
export function searchLiveEntities(
  source: SnapshotSource,
  query: string,
  limit = 8,
  feeds: readonly FeedId[] = ALL_FEEDS,
): EntityHit[] {
  const q = query.trim().toLowerCase();
  if (q.length < 2) return [];
  const scored = feeds.flatMap((feed) => {
    const snapshot = source.latest(feed);
    return snapshot ? scoreSnapshot(snapshot, q) : [];
  });
  return scored
    .sort((a, b) => a.score - b.score)
    .slice(0, limit)
    .map((s) => s.hit);
}
