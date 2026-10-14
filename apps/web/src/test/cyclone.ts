import type { Cyclone } from '../contracts';

/** Synthetic storm for browser-free UI/rendering contract tests. */
export function cyclone(overrides: Partial<Cyclone> = {}): Cyclone {
  return {
    id: 'al062026',
    name: 'TEST ALPHA',
    classification: 'Hurricane',
    basin: 'AL',
    position: { lat: 20, lon: -60 },
    positionAt: '2026-10-14T00:00:00Z',
    advisoryNumber: '6',
    issuedAt: '2026-10-13T21:00:00Z',
    intensityKt: 70,
    pressureMb: 980,
    movement: { directionDegrees: 290, speedKt: 12 },
    outlookUrl: 'https://www.nhc.noaa.gov/gtwo.php?basin=atlc',
    geometryStatus: 'current',
    geometryAdvisoryNumber: '6',
    forecastPoints: [{ position: { lat: 21, lon: -61 }, tauHours: 12, windKt: 75, gustKt: 90 }],
    track: {
      type: 'MultiLineString',
      coordinates: [
        [
          [-60, 20],
          [-61, 21],
        ],
        [
          [179, 25],
          [-179, 26],
        ],
      ],
    },
    cone: {
      type: 'MultiPolygon',
      coordinates: [
        [
          [
            [-62, 19],
            [-59, 19],
            [-59, 22],
            [-62, 19],
          ],
          [
            [-61, 20],
            [-60, 20],
            [-60, 21],
            [-61, 20],
          ],
        ],
        [
          [
            [178, 24],
            [179, 24],
            [179, 26],
            [178, 24],
          ],
        ],
      ],
    },
    ...overrides,
  };
}
