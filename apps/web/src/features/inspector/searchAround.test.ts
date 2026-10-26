import { describe, expect, it } from 'vitest';
import { aircraft, aircraftSnapshot } from '../../app/testing';
import type { FeedId, FeedSnapshot } from '../../contracts';
import { quakeSnapshot } from '../../test/fakes';
import { vessel, vesselSnapshot } from '../../test/vessel';
import {
  cardinalDirection,
  computeSearchAroundGraph,
  haversineKm,
  initialBearingDeg,
} from './searchAround';

describe('searchAround spatial graph', () => {
  it('computes accurate great-circle distance and cardinal bearings', () => {
    const tallinn = { lat: 59.437, lon: 24.7536 };
    const helsinki = { lat: 60.1699, lon: 24.9384 };
    const dist = haversineKm(tallinn, helsinki);
    expect(dist).toBeGreaterThan(80);
    expect(dist).toBeLessThan(85);

    const bearing = initialBearingDeg(tallinn, helsinki);
    expect(bearing).toBeGreaterThanOrEqual(0);
    expect(bearing).toBeLessThanOrEqual(15);
    expect(cardinalDirection(bearing)).toBe('N');
  });

  it('aggregates multi-feed proximity cohorts and ranks nearest neighbors excluding subject', () => {
    const snapshots = new Map<FeedId, FeedSnapshot>([
      [
        'aircraft',
        aircraftSnapshot([
          aircraft('4aca2d', {
            callsign: 'SAS1462',
            position: { lat: 59.437, lon: 24.7536 },
          }),
          aircraft('4ca1b2', {
            callsign: 'RYR12AB',
            position: { lat: 59.5, lon: 24.8 },
          }),
        ]),
      ],
      [
        'vessels',
        vesselSnapshot([
          vessel({
            mmsi: '276810000',
            name: 'TALLINK MYSTAR',
            position: { lat: 59.45, lon: 24.77 },
          }),
        ]),
      ],
      ['earthquakes', quakeSnapshot(['far-quake'])],
    ]);

    const graph = computeSearchAroundGraph(
      { latest: (feed) => snapshots.get(feed) },
      { feed: 'aircraft', id: '4aca2d', position: { lat: 59.437, lon: 24.7536 } },
      50,
    );

    expect(graph.totalWithinRadius).toBe(2);
    expect(graph.cohorts).toEqual([
      { feed: 'aircraft', count: 1 },
      { feed: 'vessels', count: 1 },
    ]);
    expect(graph.neighbors.map((n) => n.label)).toEqual(['TALLINK MYSTAR', 'RYR12AB']);
  });
});
