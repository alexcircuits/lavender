import { describe, expect, it } from 'vitest';
import { CITY_PRESETS, cityPresetById } from './cityPois';

describe('cityPois', () => {
  it('contains all 9 tactical cities with 5 landmarks each inside valid bounding boxes', () => {
    expect(CITY_PRESETS).toHaveLength(9);
    for (const city of CITY_PRESETS) {
      expect(city.pois).toHaveLength(5);
      expect(city.bbox.west).toBeLessThan(city.bbox.east);
      expect(city.bbox.south).toBeLessThan(city.bbox.north);
      for (const poi of city.pois) {
        expect(poi.position.lat).toBeGreaterThanOrEqual(city.bbox.south);
        expect(poi.position.lat).toBeLessThanOrEqual(city.bbox.north);
        expect(poi.position.lon).toBeGreaterThanOrEqual(city.bbox.west);
        expect(poi.position.lon).toBeLessThanOrEqual(city.bbox.east);
      }
    }
  });

  it('finds preset cities by id', () => {
    expect(cityPresetById('tallinn')?.name).toBe('Tallinn');
    expect(cityPresetById('dc')?.pois[3]?.name).toBe('Pentagon');
    expect(cityPresetById('unknown')).toBeUndefined();
  });
});
