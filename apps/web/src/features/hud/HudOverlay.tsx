import { useEffect, useMemo, useState } from 'react';
import type { GeoPoint } from '../../contracts';
import {
  calculateGsd,
  calculateNiirs,
  calculateOna,
  estimateSunElevation,
} from '../../globe/renderers/wasmOptics';
import { CITY_PRESETS } from '../../lib/cityPois';
import { formatDms } from '../../lib/coordinates';
import { spaceMgrs, toMgrs } from '../../lib/mgrs';
import { cardinalDirection, haversineKm, initialBearingDeg } from '../inspector/searchAround';
import styles from './hud.module.css';

export type Classification =
  | 'TOP SECRET // SI / TK // NOFORN'
  | 'SECRET // NOFORN'
  | 'CONFIDENTIAL // NOFORN'
  | 'UNCLASSIFIED // FOUO'
  | 'UNCLASSIFIED';

export type HudLayoutVariant = 'tactical' | 'operator' | 'minimal';

export type ViewBand = 'STREET' | 'CITY' | 'METRO' | 'REGIONAL' | 'GLOBAL';

export function getViewBand(altM: number): ViewBand {
  if (altM < 1500) return 'STREET';
  if (altM < 10000) return 'CITY';
  if (altM < 60000) return 'METRO';
  if (altM < 250000) return 'REGIONAL';
  return 'GLOBAL';
}

export function findNearestCity(point: GeoPoint): {
  name: string;
  distanceKm: number;
  bearingDeg: number;
  cardinal: string;
} | null {
  let best: {
    name: string;
    distanceKm: number;
    bearingDeg: number;
    cardinal: string;
  } | null = null;

  for (const preset of CITY_PRESETS) {
    const center: GeoPoint = {
      lat: (preset.bbox.south + preset.bbox.north) / 2,
      lon: (preset.bbox.west + preset.bbox.east) / 2,
    };
    const dist = haversineKm(point, center);
    if (!best || dist < best.distanceKm) {
      const brng = initialBearingDeg(point, center);
      best = {
        name: preset.name,
        distanceKm: dist,
        bearingDeg: brng,
        cardinal: cardinalDirection(brng),
      };
    }
  }
  return best;
}

export interface HudOverlayProps {
  readonly className?: string | undefined;
  readonly visible?: boolean | undefined;
  readonly theme?: 'noir' | 'nvg' | 'flir' | 'crt' | undefined;
  readonly layout?: HudLayoutVariant | undefined;
  readonly classification?: Classification | undefined;
  readonly cameraPosition?: GeoPoint | undefined;
  readonly cameraAltitudeM?: number | undefined;
  readonly cameraPitchDeg?: number | undefined;
  readonly cameraHeadingDeg?: number | undefined;
}

/**
 * Reconnaissance Intelligence HUD Overlay (NRO/NGA satellite reconnaissance aesthetic).
 * Renders authentic reconnaissance telemetry (MGRS, DMS, GSD, NIIRS, ONA, Sun, View Band,
 * Nearest City Vector, and Classification Banner) over the Cesium viewport.
 */
export function HudOverlay({
  className,
  visible = true,
  theme = 'noir',
  layout = 'tactical',
  classification = 'TOP SECRET // SI / TK // NOFORN',
  cameraPosition = { lat: 38.8977, lon: -77.0365 },
  cameraAltitudeM = 15000,
  cameraPitchDeg = -45,
  cameraHeadingDeg = 0,
}: HudOverlayProps) {
  const [now, setNow] = useState(() => new Date());

  useEffect(() => {
    if (!visible) return;
    const interval = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(interval);
  }, [visible]);

  const viewBand = getViewBand(cameraAltitudeM);
  const gsdM = calculateGsd(cameraAltitudeM);
  const niirs = calculateNiirs(gsdM);
  const ona = calculateOna(cameraPitchDeg);
  const unixSec = Math.floor(now.getTime() / 1000);
  const sunElev = estimateSunElevation(cameraPosition.lat, cameraPosition.lon, unixSec);

  const rawMgrs = toMgrs(cameraPosition.lat, cameraPosition.lon);
  const mgrsText = rawMgrs ? spaceMgrs(rawMgrs) : 'N/A';
  const dmsText = formatDms(cameraPosition);

  const nearestCity = useMemo(() => findNearestCity(cameraPosition), [cameraPosition]);

  if (!visible) return null;

  const themeClass =
    theme === 'nvg'
      ? styles.themeNvg
      : theme === 'flir'
        ? styles.themeFlir
        : theme === 'crt'
          ? styles.themeCrt
          : styles.themeNoir;

  const timeUtc = `${now.toISOString().slice(11, 19)}Z`;

  return (
    <div
      data-testid="hud-overlay"
      className={`${styles.overlay} ${themeClass} ${className ?? ''}`}
      aria-hidden="true"
    >
      {/* Classification Banner */}
      <div className={styles.classificationBanner}>{classification}</div>

      {layout !== 'minimal' && (
        <>
          {/* Top-Left Telemetry Block */}
          <div className={`${styles.corner} ${styles.topLeft}`}>
            <div className={styles.row}>
              <span className={styles.label}>BAND:</span>
              <span className={styles.value}>{viewBand}</span>
            </div>
            <div className={styles.row}>
              <span className={styles.label}>COLL:</span>
              <span className={styles.value}>{timeUtc}</span>
            </div>
            <div className={styles.row}>
              <span className={styles.label}>HDG:</span>
              <span className={styles.value}>
                {Math.round(((cameraHeadingDeg % 360) + 360) % 360)
                  .toString()
                  .padStart(3, '0')}
                °
              </span>
            </div>
          </div>

          {/* Top-Right Telemetry Block */}
          <div className={`${styles.corner} ${styles.topRight}`}>
            <div className={styles.row}>
              <span className={styles.label}>GSD:</span>
              <span className={styles.value}>{gsdM.toFixed(2)}m</span>
              <span className={styles.label} style={{ marginLeft: 6 }}>
                NIIRS:
              </span>
              <span className={styles.value}>{niirs.toFixed(1)}</span>
            </div>
            <div className={styles.row}>
              <span className={styles.label}>SUN:</span>
              <span className={styles.value}>{sunElev.toFixed(1)}° EL</span>
            </div>
            <div className={styles.row}>
              <span className={styles.label}>ONA:</span>
              <span className={styles.value}>{ona.toFixed(1)}°</span>
            </div>
          </div>
        </>
      )}

      {/* Bottom-Left Vector Block */}
      <div className={`${styles.corner} ${styles.bottomLeft}`}>
        {nearestCity && (
          <div className={styles.row}>
            <span className={styles.label}>LOC:</span>
            <span className={styles.value}>
              {nearestCity.name.toUpperCase()} | {nearestCity.distanceKm.toFixed(1)}km |{' '}
              {nearestCity.bearingDeg.toString().padStart(3, '0')}° ({nearestCity.cardinal})
            </span>
          </div>
        )}
        <div className={styles.row}>
          <span className={styles.label}>ALT:</span>
          <span className={styles.value}>{Math.round(cameraAltitudeM).toLocaleString()}m MSL</span>
        </div>
      </div>

      {/* Bottom-Right Coordinates Block */}
      <div className={`${styles.corner} ${styles.bottomRight}`}>
        <div className={styles.row}>
          <span className={styles.label}>MGRS:</span>
          <span className={styles.value}>{mgrsText}</span>
        </div>
        <div className={styles.row}>
          <span className={styles.value}>{dmsText}</span>
        </div>
      </div>

      {/* Tactical Center Reticle */}
      {layout === 'tactical' && (
        <div className={styles.reticle}>
          <div className={styles.crosshairH} />
          <div className={styles.crosshairV} />
          <div className={styles.centerPip} />
        </div>
      )}
    </div>
  );
}
