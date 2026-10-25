import { describe, expect, it } from 'vitest';
import { exportAnnotationsToGeoJson, parseGeoJsonToAnnotations } from './geojson';
import type { MapAnnotation } from './model';

describe('annotations GeoJSON import/export', () => {
  it('round-trips pin, line, and area annotations through GeoJSON FeatureCollection', () => {
    const source: readonly MapAnnotation[] = [
      {
        id: 'pin-1',
        kind: 'pin',
        points: [{ lat: 59.437, lon: 24.7536 }],
        label: 'Tallinn HQ',
        note: 'Primary relay',
        createdAt: '2026-10-25T12:00:00.000Z',
        approximate: true,
      },
      {
        id: 'line-1',
        kind: 'line',
        points: [
          { lat: 59.437, lon: 24.7536 },
          { lat: 60.1699, lon: 24.9384 },
        ],
        label: 'Ferry corridor',
        note: 'Helsinki link',
        createdAt: '2026-10-25T12:00:00.000Z',
        approximate: true,
      },
      {
        id: 'area-1',
        kind: 'area',
        points: [
          { lat: 59.4, lon: 24.7 },
          { lat: 59.5, lon: 24.7 },
          { lat: 59.5, lon: 24.8 },
        ],
        label: 'Harbour sector',
        note: '',
        createdAt: '2026-10-25T12:00:00.000Z',
        approximate: true,
      },
    ];

    const json = exportAnnotationsToGeoJson(source);
    const imported = parseGeoJsonToAnnotations(json, 0);

    expect(imported).toHaveLength(3);
    expect(imported[0]).toMatchObject({
      kind: 'pin',
      label: 'Tallinn HQ',
      note: 'Primary relay',
      points: [{ lat: 59.437, lon: 24.7536 }],
    });
    expect(imported[1]).toMatchObject({
      kind: 'line',
      label: 'Ferry corridor',
      points: [
        { lat: 59.437, lon: 24.7536 },
        { lat: 60.1699, lon: 24.9384 },
      ],
    });
    expect(imported[2]).toMatchObject({
      kind: 'area',
      label: 'Harbour sector',
      points: [
        { lat: 59.4, lon: 24.7 },
        { lat: 59.5, lon: 24.7 },
        { lat: 59.5, lon: 24.8 },
      ],
    });
  });

  it('rejects malformed JSON or unsupported empty FeatureCollections', () => {
    expect(() => parseGeoJsonToAnnotations('{bad')).toThrow(/Invalid JSON/);
    expect(() =>
      parseGeoJsonToAnnotations(JSON.stringify({ type: 'FeatureCollection', features: [] })),
    ).toThrow(/No supported/);
  });
});
