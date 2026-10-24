import { describe, expect, it } from 'vitest';
import {
  BOUNDARY_LEVELS,
  boundaryDetailBand,
  boundaryZoomDefaults,
  boundaryZoomForHeightM,
} from './boundaryHierarchy';

describe('boundary hierarchy', () => {
  it('serves admin-0 classes and marks deeper levels unserved', () => {
    const admin0 = BOUNDARY_LEVELS[0];
    const admin1 = BOUNDARY_LEVELS[1];
    const admin2 = BOUNDARY_LEVELS[2];
    expect(admin0?.served).toBe(true);
    expect(admin0?.classes).toHaveLength(4);
    expect(admin1?.served).toBe(false);
    expect(admin2?.served).toBe(false);
    // Every unserved level explains itself: no silent gaps in the drill-down.
    for (const level of BOUNDARY_LEVELS.filter((l) => !l.served)) {
      expect(level.note.length).toBeGreaterThan(10);
      expect(level.classes).toEqual([]);
    }
  });

  it('maps camera height to a clamped zoom level', () => {
    // Continental view height (~10,000 km) is a low zoom; a city block (~2 km) is high.
    expect(boundaryZoomForHeightM(10_000_000)).toBeLessThanOrEqual(6);
    expect(boundaryZoomForHeightM(2_000)).toBeGreaterThanOrEqual(12);
    // Nonsense heights clamp instead of NaN.
    expect(boundaryZoomForHeightM(Number.NaN)).toBeGreaterThanOrEqual(0);
    expect(boundaryZoomForHeightM(Number.NaN)).toBeLessThanOrEqual(18);
    expect(boundaryZoomForHeightM(-5)).toBeGreaterThanOrEqual(0);
  });

  it('drops maritime lines on world views and shows them once zoomed in', () => {
    expect(boundaryDetailBand(2)).toBe('world');
    expect(boundaryDetailBand(5)).toBe('close');
    expect(boundaryZoomDefaults('world').maritimeIndicative).toBe(false);
    expect(boundaryZoomDefaults('close').maritimeIndicative).toBe(true);
    for (const band of ['world', 'close'] as const) {
      const defaults = boundaryZoomDefaults(band);
      expect(defaults.internationalLand).toBe(true);
      expect(defaults.disputedLine).toBe(true);
      expect(defaults.breakawayArea).toBe(true);
    }
  });
});
