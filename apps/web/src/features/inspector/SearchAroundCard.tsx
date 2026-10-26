import { useMemo, useState } from 'react';
import type { FeedId, GeoPoint } from '../../contracts';
import { Badge, Button, SegmentedControl } from '../../design';
import { useFeeds, useGlobe, useRuntime, useWorkspace } from '../../runtime';
import styles from './inspector.module.css';
import { computeSearchAroundGraph, type ProximityNeighbor } from './searchAround';

export interface SearchAroundCardProps {
  readonly feed: string;
  readonly entityId: string;
  readonly position: GeoPoint;
}

type RadiusOption = '50' | '150' | '500';

const RADIUS_OPTIONS: readonly { value: RadiusOption; label: string }[] = [
  { value: '50', label: '50 km' },
  { value: '150', label: '150 km' },
  { value: '500', label: '500 km' },
];

function nodeFamily(feed: FeedId): 'air' | 'maritime' | 'hazard' | 'context' {
  switch (feed) {
    case 'aircraft':
      return 'air';
    case 'vessels':
      return 'maritime';
    case 'earthquakes':
    case 'cyclones':
    case 'fires':
    case 'perimeters':
      return 'hazard';
    default:
      return 'context';
  }
}

export function SearchAroundCard({ feed, entityId, position }: SearchAroundCardProps) {
  const runtime = useRuntime();
  const globe = useGlobe();
  const select = useWorkspace((s) => s.select);
  const summaries = useFeeds((s) => s.summaries);
  const [radiusOpt, setRadiusOpt] = useState<RadiusOption>('150');
  const radiusKm = Number(radiusOpt);

  // biome-ignore lint/correctness/useExhaustiveDependencies: summaries triggers recomputation when live feed snapshots arrive.
  const graph = useMemo(
    () => computeSearchAroundGraph(runtime.feeds, { feed, id: entityId, position }, radiusKm),
    [runtime.feeds, feed, entityId, position.lat, position.lon, radiusKm, summaries],
  );

  const pivotTo = (neighbor: ProximityNeighbor) => {
    select({ feed: neighbor.feed, id: neighbor.id });
    globe?.flyTo({ kind: 'point', position: neighbor.position });
  };

  return (
    <section className={styles.sectionCard} aria-label="Search-Around & Co-Location Graph">
      <div className={styles.cardHeader}>
        <h4 className={styles.cardTitle}>Search-Around &amp; Co-Location Graph</h4>
        <Badge>{graph.totalWithinRadius} nearby</Badge>
      </div>

      <div className={styles.graphLayout}>
        <SegmentedControl<RadiusOption>
          aria-label="Search-around radius"
          size="sm"
          options={RADIUS_OPTIONS}
          value={radiusOpt}
          onValueChange={setRadiusOpt}
        />

        <div className={styles.radarRow}>
          <svg
            viewBox="0 0 112 112"
            className={styles.radarSvg}
            role="img"
            aria-label={`Co-location radar within ${radiusKm} km`}
          >
            <circle cx="56" cy="56" r="44" className={styles.radarRing} />
            <circle cx="56" cy="56" r="22" className={styles.radarRing} />
            <line x1="56" y1="8" x2="56" y2="104" className={styles.radarAxis} />
            <line x1="8" y1="56" x2="104" y2="56" className={styles.radarAxis} />
            {graph.neighbors.map((n) => {
              const norm = Math.min(1, Math.max(0.12, n.distanceKm / radiusKm));
              const rad = ((n.bearingDeg - 90) * Math.PI) / 180;
              const x = 56 + Math.cos(rad) * norm * 42;
              const y = 56 + Math.sin(rad) * norm * 42;
              return (
                <g key={`${n.feed}:${n.id}`}>
                  <line x1="56" y1="56" x2={x} y2={y} className={styles.radarEdge} />
                  <circle
                    cx={x}
                    cy={y}
                    r="4"
                    className={styles.radarNode}
                    data-family={nodeFamily(n.feed)}
                  />
                </g>
              );
            })}
            <circle cx="56" cy="56" r="4.5" className={styles.radarHub} />
          </svg>

          <div className={styles.cohortSummary}>
            <span className={styles.subtext}>
              {graph.totalWithinRadius === 0
                ? `No other live contacts within ${radiusKm} km.`
                : `${graph.totalWithinRadius} live contact${graph.totalWithinRadius === 1 ? '' : 's'} within ${radiusKm} km across ${graph.cohorts.length} feed${graph.cohorts.length === 1 ? '' : 's'}.`}
            </span>
            {graph.cohorts.length > 0 ? (
              <div className={styles.cohortChips}>
                {graph.cohorts.map((c) => (
                  <Badge key={c.feed}>
                    {c.feed}: {c.count}
                  </Badge>
                ))}
              </div>
            ) : null}
          </div>
        </div>

        {graph.neighbors.length > 0 ? (
          <ul className={styles.neighborList} aria-label="Nearby co-located entities">
            {graph.neighbors.map((n) => (
              <li key={`${n.feed}:${n.id}`} className={styles.neighborItem}>
                <div className={styles.neighborInfo}>
                  <span className={styles.neighborLabel}>
                    {n.label} · {n.feed}
                  </span>
                  <span className={styles.neighborMeta}>
                    {n.distanceKm.toFixed(1)} km · {String(n.bearingDeg).padStart(3, '0')}°{' '}
                    {n.cardinal} · {n.detail}
                  </span>
                </div>
                <Button size="sm" variant="secondary" onClick={() => pivotTo(n)}>
                  Pivot to {n.label}
                </Button>
              </li>
            ))}
          </ul>
        ) : null}
      </div>
    </section>
  );
}
