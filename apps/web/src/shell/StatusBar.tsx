import { useEffect, useRef, useState } from 'react';
import type { GeoPoint } from '../contracts';
import { CircleIcon, cx, formatCount, type Icon, SquareIcon, TriangleIcon } from '../design';
import type { ViewState } from '../globe';
import { formatDecimal, formatDms } from '../lib/coordinates';
import { spaceMgrs, toMgrs } from '../lib/mgrs';
import { useNow } from '../lib/useNow';
import type { TransportState } from '../platform';
import { useFeeds, useGlobe, usePreferences, useSystem, useWorkspace } from '../runtime';
import type { UnitSystem } from '../state/preferencesStore';
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

/** Pointer position under the cursor, formatted per preferences. Local state: it changes per frame. */
function CursorReadout() {
  const globe = useGlobe();
  const format = usePreferences((s) => s.coordinateFormat);
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

  let formatted = '';
  let mgrsSuffix = '';
  if (format === 'dms') {
    formatted = formatDms(position);
  } else if (format === 'mgrs') {
    const mgrs = toMgrs(position.lat, position.lon, 5);
    formatted = mgrs ? spaceMgrs(mgrs) : formatDecimal(position);
  } else {
    formatted = formatDecimal(position);
    const mgrs = toMgrs(position.lat, position.lon, 5);
    if (mgrs) {
      mgrsSuffix = ` · ${spaceMgrs(mgrs)}`;
    }
  }

  return (
    <span className={cx(styles.cursor, 'mono')} data-testid="cursor-position">
      <span className="visually-hidden">Pointer at </span>
      {formatted}
      {mgrsSuffix}
    </span>
  );
}

/** Formats camera altitude respecting the user's unit system and optional datum indicator. */
export function formatHeight(
  metres: number,
  units: UnitSystem = 'metric',
  datum?: string | undefined,
): string {
  if (!Number.isFinite(metres)) return 'unknown';
  const datumSuffix = datum ? ` ${datum}` : '';
  if (units === 'metric') {
    if (metres < 1_000) return `${Math.round(metres)} m${datumSuffix}`;
    const km = metres / 1_000;
    if (km < 100) return `${km.toFixed(1)} km${datumSuffix}`;
    return `${formatCount(km)} km${datumSuffix}`;
  }
  const feet = metres * 3.28084;
  if (feet < 10_000) return `${Math.round(feet).toLocaleString('en-US')} ft${datumSuffix}`;
  if (units === 'nautical') {
    const nm = metres / 1852;
    if (nm < 100) return `${nm.toFixed(1)} NM${datumSuffix}`;
    return `${formatCount(nm)} NM${datumSuffix}`;
  }
  const miles = metres / 1609.344;
  if (miles < 100) return `${miles.toFixed(1)} mi${datumSuffix}`;
  return `${formatCount(miles)} mi${datumSuffix}`;
}

function CameraHeight() {
  const globe = useGlobe();
  const unitSystem = usePreferences((s) => s.unitSystem);
  const [view, setView] = useState<ViewState | null>(null);
  const [datum, setDatum] = useState<'msl' | 'hae'>('msl');
  const [geoidReady, setGeoidReady] = useState(false);
  const geoidRef = useRef<typeof import('../globe/geoid') | null>(null);

  useEffect(() => {
    let active = true;
    import('../globe/geoid')
      .then((mod) => {
        geoidRef.current = mod;
        return mod.ensureGeoidReady();
      })
      .then(() => {
        if (active) setGeoidReady(true);
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    if (!globe) return;
    const off = globe.onViewChange(setView);
    return () => {
      off();
      setView(null);
    };
  }, [globe]);

  if (!view || !Number.isFinite(view.heightM)) return null;

  const rawHeightM = view.heightM;
  let displayHeightM = rawHeightM;
  let datumLabel: string | undefined;

  if (datum === 'msl' && geoidReady && view.centre && geoidRef.current) {
    try {
      const n = geoidRef.current.geoidHeight(view.centre.lat, view.centre.lon);
      displayHeightM = geoidRef.current.ellipsoidalToMslDisplayM(rawHeightM, n);
      datumLabel = 'MSL';
    } catch {
      displayHeightM = rawHeightM;
    }
  } else if (datum === 'hae' && geoidReady && view.centre) {
    datumLabel = 'HAE';
  }

  const toggleDatum = () => {
    setDatum((prev) => (prev === 'msl' ? 'hae' : 'msl'));
  };

  return (
    <button
      type="button"
      className={cx(styles.altitude, styles.altitudeButton)}
      onClick={toggleDatum}
      title="Click to toggle altitude datum (MSL / HAE)"
    >
      Camera at {formatHeight(displayHeightM, unitSystem, datumLabel)}
    </button>
  );
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
