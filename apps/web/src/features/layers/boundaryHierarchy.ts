/**
 * The administrative division hierarchy the boundaries layer can actually serve. The current
 * boundary contract (`BoundaryFeature`) carries no admin-level field: the bundled Natural
 * Earth 50m pack is admin-0 only, and the four `BoundaryClass` values classify admin-0 lines
 * (land, disputed, breakaway, maritime) rather than division depth. The drill-down therefore
 * presents the served admin-0 classes under their level and marks deeper levels (admin-1
 * states/provinces, counties) explicitly as not served instead of implying data that is absent.
 */
import type { BoundaryClass } from '../../contracts';
import type { BoundaryClassVisibility } from '../../globe';

export type BoundaryLevelId = 'admin0' | 'admin1' | 'admin2';

export interface BoundaryLevel {
  readonly id: BoundaryLevelId;
  readonly label: string;
  readonly shortLabel: string;
  /** Whether the current boundary endpoint returns features for this level. */
  readonly served: boolean;
  /** Boundary classes served at this level (empty for unserved levels). */
  readonly classes: readonly BoundaryClass[];
  /** Plain-language note shown for unserved levels. */
  readonly note: string;
}

/** Drill-down order: country lines first, deeper divisions where (if ever) served. */
export const BOUNDARY_LEVELS: readonly BoundaryLevel[] = [
  {
    id: 'admin0',
    label: 'Countries (admin-0)',
    shortLabel: 'admin-0',
    served: true,
    classes: ['internationalLand', 'disputedLine', 'breakawayArea', 'maritimeIndicative'],
    note: 'Natural Earth 50m de facto country boundaries.',
  },
  {
    id: 'admin1',
    label: 'States & provinces (admin-1)',
    shortLabel: 'admin-1',
    served: false,
    classes: [],
    note: 'The current boundary source serves admin-0 only; no admin-1 lines are returned.',
  },
  {
    id: 'admin2',
    label: 'Counties (admin-2)',
    shortLabel: 'admin-2',
    served: false,
    classes: [],
    note: 'The current boundary source serves admin-0 only; no county lines are returned.',
  },
];

/** Camera height (m) at the equator for a web-mercator zoom level; approximate is enough. */
const EQUATORIAL_TILE_SPAN_M = 591_657_550.5;

/** The zoom level the boundary endpoint simplifies for, from the camera height. */
export function boundaryZoomForHeightM(heightM: number): number {
  const safe = Number.isFinite(heightM) && heightM > 0 ? heightM : EQUATORIAL_TILE_SPAN_M / 16;
  const zoom = Math.round(Math.log2(EQUATORIAL_TILE_SPAN_M / safe));
  return Math.min(18, Math.max(0, zoom));
}

/** Detail bands with different class defaults; world views drop the maritime noise. */
export type BoundaryDetailBand = 'world' | 'close';

export function boundaryDetailBand(zoom: number): BoundaryDetailBand {
  return zoom <= 3 ? 'world' : 'close';
}

/** Zoom-appropriate class defaults: maritime indicative lines appear only once zoomed in. */
export function boundaryZoomDefaults(band: BoundaryDetailBand): BoundaryClassVisibility {
  if (band === 'world') {
    return {
      internationalLand: true,
      disputedLine: true,
      breakawayArea: true,
      maritimeIndicative: false,
    };
  }
  return {
    internationalLand: true,
    disputedLine: true,
    breakawayArea: true,
    maritimeIndicative: true,
  };
}
