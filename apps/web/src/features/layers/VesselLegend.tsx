import { useEffect, useState } from 'react';
import { formatCount } from '../../design';
import { useFeeds, useGlobe } from '../../runtime';
import styles from './layers.module.css';

/** Matches AT_REST_KNOTS in globe/renderers/vesselSymbology.ts (checked by the legend test). */
export const AT_REST_LABEL = 'At rest (under 0.5 kn) or speed not reported';
export const UNDERWAY_LABEL = 'Under way, turned to its course (or heading)';
/** Matches THIN_MIN_HEIGHT_M in globe/renderers/vesselSymbology.ts (checked by the test). */
export const THINNING_ABOVE_M = 1_000_000;
export const THINNING_NOTE =
  'Wide views draw one ship per crowded spot, moving ships first. The count is every ship in view.';
export const THINNING_NOW =
  'This view is wide: crowded spots show one ship each, moving ships first. The count is every ship in view.';
export const ALL_DRAWN_NOW = 'Every ship in view is drawn at this zoom.';

/** Whether the globe's settled camera is high enough for the renderer to thin ships. */
function useWideView(): boolean | null {
  const globe = useGlobe();
  const [heightM, setHeightM] = useState<number | null>(() => globe?.cameraPose()?.heightM ?? null);
  useEffect(() => {
    if (!globe) return;
    setHeightM(globe.cameraPose()?.heightM ?? null);
    return globe.onViewChange((view) => setHeightM(view.heightM));
  }, [globe]);
  return heightM === null ? null : heightM > THINNING_ABOVE_M;
}

/** The globe's hull chevron, bow up (vesselGlyphs.ts, scaled). */
function ChevronSwatch() {
  return (
    <svg className={styles.vesselSwatch} viewBox="0 0 18 18" width="16" height="16" aria-hidden>
      <path d="M9 1 13.5 8v8.5L9 13.5 4.5 16.5V8Z" />
    </svg>
  );
}

function DotSwatch() {
  return (
    <svg className={styles.vesselSwatch} viewBox="0 0 18 18" width="16" height="16" aria-hidden>
      <circle cx="9" cy="9" r="5" />
    </svg>
  );
}

/**
 * The vessels layer's key (design-system §4: hull chevron rotated to course, dot at rest), the
 * honest counts from the latest snapshot and how wide views reduce clutter.
 */
export function VesselLegend() {
  const summary = useFeeds((s) => s.summaries.vessels);
  const wide = useWideView();
  const hasData =
    summary !== undefined &&
    (summary.phase === 'ready' || (summary.phase === 'error' && summary.fetchedAt !== null));
  return (
    <div className={styles.optionStack}>
      <ul className={styles.legend} aria-label="Vessel symbol legend">
        <li>
          <ChevronSwatch />
          {UNDERWAY_LABEL}
        </li>
        <li>
          <DotSwatch />
          {AT_REST_LABEL}
        </li>
      </ul>
      {hasData ? (
        <p className={styles.legendNote}>
          {formatCount(summary.count)} in view · {formatCount(summary.sourceCount)} known to the
          server
        </p>
      ) : null}
      <p className={styles.legendNote}>
        {wide === null ? THINNING_NOTE : wide ? THINNING_NOW : ALL_DRAWN_NOW}
      </p>
    </div>
  );
}
