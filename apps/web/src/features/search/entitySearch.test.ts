import { describe, expect, it } from 'vitest';
import { aircraft, aircraftSnapshot, satellite, satelliteSnapshot } from '../../app/testing';
import type { FeedId, FeedSnapshot } from '../../contracts';
import { quakeSnapshot } from '../../test/fakes';
import { vessel, vesselSnapshot } from '../../test/vessel';
import { searchLiveEntities } from './entitySearch';

function source(snapshots: FeedSnapshot[]) {
  const byFeed = new Map<FeedId, FeedSnapshot>(snapshots.map((s) => [s.feed, s]));
  return { latest: (feed: FeedId) => byFeed.get(feed) };
}

describe('searchLiveEntities', () => {
  const feeds = source([
    aircraftSnapshot([
      aircraft('4aca2d', { callsign: 'SAS1462', typeCode: 'A20N', altitudeBaroFt: 36_000 }),
      aircraft('461f2a', { callsign: 'FIN7LY', registration: 'OH-LWA' }),
      aircraft('abcdef', { callsign: 'XSAS99' }),
    ]),
    satelliteSnapshot([satellite(25544), satellite(48274, { name: 'CSS (TIANHE)' })]),
    quakeSnapshot(['us7000abcd']),
  ]);

  it('matches callsigns, ICAO addresses and registrations, best first', () => {
    const hits = searchLiveEntities(feeds, 'sas');
    expect(hits.map((h) => h.label)).toEqual(['SAS1462', 'XSAS99']);
    expect(hits[0]).toMatchObject({ feed: 'aircraft', id: '4aca2d', position: { lat: 60.1699 } });
    expect(hits[0]?.detail).toMatch(/^A20N · 36.000 ft$/);
    expect(searchLiveEntities(feeds, '461f2a')[0]?.label).toBe('FIN7LY');
    expect(searchLiveEntities(feeds, 'oh-lwa')[0]?.label).toBe('FIN7LY');
  });

  it('matches satellites by name and NORAD id, and quakes by event id', () => {
    expect(searchLiveEntities(feeds, '25544')[0]).toMatchObject({
      feed: 'satellites',
      label: 'ISS (ZARYA)',
    });
    expect(searchLiveEntities(feeds, 'tianhe')[0]?.id).toBe('48274');
    expect(searchLiveEntities(feeds, 'us7000')[0]?.feed).toBe('earthquakes');
  });

  it('ignores one-letter queries and caps the result count', () => {
    expect(searchLiveEntities(feeds, 's')).toEqual([]);
    expect(searchLiveEntities(feeds, 'ss', 1)).toHaveLength(1);
  });

  it('matches loaded vessels by MMSI, IMO and case-insensitive name', () => {
    const loaded = source([
      vesselSnapshot([
        vessel({ name: ' TEST SHIP ', imo: '7654321' }),
        vessel({ mmsi: '987654321', name: 'TEST SHIP' }),
        vessel({ mmsi: '234567890', name: ' ' }),
      ]),
    ]);
    expect(searchLiveEntities(loaded, '123456789')).toEqual([
      {
        feed: 'vessels',
        id: '123456789',
        label: 'TEST SHIP',
        detail: 'MMSI 123456789',
        position: { lat: 60.1, lon: 24.9 },
      },
    ]);
    expect(searchLiveEntities(loaded, '7654321')[0]?.id).toBe('123456789');
    expect(searchLiveEntities(loaded, 'test ship').map((hit) => hit.id)).toEqual([
      '123456789',
      '987654321',
    ]);
    expect(searchLiveEntities(loaded, '234567890')[0]?.label).toBe('234567890');
    expect(searchLiveEntities(loaded, 'test ship', 8, ['aircraft'])).toEqual([]);
  });
});
