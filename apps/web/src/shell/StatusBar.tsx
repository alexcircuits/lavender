import { useEffect, useState } from 'react';
import type { GeoPoint } from '../contracts';
import { CircleIcon, cx, formatCount, type Icon, SquareIcon, TriangleIcon } from '../design';
import { formatDecimal } from '../lib/coordinates';
import { spaceMgrs, toMgrs } from '../lib/mgrs';
import { useNow } from '../lib/useNow';
import type { TransportState } from '../platform';
import { useFeeds, useGlobe, useSystem, useWorkspace } from '../runtime';
import { feedHealth } from './feedHealth';
import styles from './shell.module.css';

export const TRANSPORT_WORDS: Readonly<Record<TransportState, string>> = {
  connecting: 'Connecting…',
  live: 'Connected',
  polling: 'Live stream down · polling',
  offline: 'Not connected',
};

const TRANSPORT_GLYPH: Readonly<
  Record<TransportState, { icon: Icon; weight: 'fill' | 'bold'; tone: string }>
> = {
  connecting: { icon: CircleIcon, weight: 'bold', tone: 'neutral' },
  live: { icon: CircleIcon, weight: 'fill', tone: 'good' },
  polling: { icon: TriangleIcon, weight: 'fill', tone: 'warning' },
  offline: { icon: SquareIcon, weight: 'fill', tone: 'alert' },
};

function Connection() {
  const transport = useFeeds((s) => s.transport);
  const summaries = useFeeds((s) => s.summaries);
  const layers = useWorkspace((s) => s.layers);
  const catalog = useSystem((s) => s.catalog);
  const health = feedHealth(catalog, layers, summaries);
  const live = health.counts.live ?? 0;
  const glyph = TRANSPORT_GLYPH[transport];
  const Glyph = glyph.icon;
  return (
    <span className={styles.connection} data-tone={glyph.tone}>
      <Glyph size={10} weight={glyph.weight} aria-hidden />
      <span>{TRANSPORT_WORDS[transport]}</span>
      {catalog ? (
        <span className={styles.liveCount}>
          {' · '}
          {health.active === 0
            ? 'No feeds on'
            : `${formatCount(live)} ${live === 1 ? 'feed' : 'feeds'} live`}
        </span>
      ) : null}
    </span>
  );
}

/** Pointer position under the cursor, decimal and MGRS. Local state: it changes per frame. */
function CursorReadout() {
  const globe = useGlobe();
  const [position, setPosition] = useState<GeoPoint | null>(null);
  useEffect(() => {
    if (!globe) return;
    const off = globe.onCursor(setPosition);
    return () => {
      off();
      setPosition(null);
    };
  }, [globe]);
  if (!position) return <span className={styles.cursor} />;
  const mgrs = toMgrs(position.lat, position.lon, 5);
  return (
    <span className={cx(styles.cursor, 'mono')} data-testid="cursor-position">
      <span className="visually-hidden">Pointer at </span>
      {formatDecimal(position)}
      {mgrs ? ` · ${spaceMgrs(mgrs)}` : ''}
    </span>
  );
}

/** "850 m", "12.4 km", "12 400 km". */
export function formatHeight(metres: number): string {
  if (!Number.isFinite(metres)) return 'unknown';
  if (metres < 1_000) return `${Math.round(metres)} m`;
  const km = metres / 1_000;
  if (km < 100) return `${km.toFixed(1)} km`;
  return `${formatCount(km)} km`;
}

function CameraHeight() {
  const globe = useGlobe();
  const [heightM, setHeightM] = useState<number | null>(null);
  useEffect(() => {
    if (!globe) return;
    const off = globe.onViewChange((view) => setHeightM(view.heightM));
    return () => {
      off();
      setHeightM(null);
    };
  }, [globe]);
  if (heightM === null) return null;
  return <span className={styles.altitude}>Camera at {formatHeight(heightM)}</span>;
}

export function UtcClock({ className }: { className?: string | undefined }) {
  const now = useNow();
  const iso = new Date(now).toISOString();
  return (
    <time className={cx(styles.clock, className)} dateTime={iso}>
      {iso.slice(11, 19)} UTC
    </time>
  );
}

export interface StatusBarProps {
  readonly className?: string | undefined;
}

/**
 * The 28 px status bar (design-system §6): connection in words and how many feeds are live,
 * the pointer position (decimal + MGRS), camera height and the UTC clock.
 */
export function StatusBar({ className }: StatusBarProps) {
  return (
    <footer className={cx(styles.statusBar, className)}>
      <Connection />
      <span className={styles.statusMiddle}>
        <CursorReadout />
        <CameraHeight />
      </span>
      <UtcClock />
    </footer>
  );
}
