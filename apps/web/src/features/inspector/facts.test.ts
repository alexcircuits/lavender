import { describe, expect, it } from 'vitest';
import { aircraft, satellite } from '../../app/testing';
import type { Earthquake, FireCell, VesselEta } from '../../contracts';
import { formatCount } from '../../design';
import { isValidFireCell } from '../../feeds/fireCells';
import { vessel } from '../../test/vessel';
import {
  aircraftFacts,
  earthquakeFacts,
  type Fact,
  fireFacts,
  formatVesselDimensions,
  formatVesselEta,
  inspect,
  NOT_REPORTED,
  provenanceFacts,
  satelliteFacts,
  trimmed,
  vesselFacts,
  vesselVoyageFacts,
} from './facts';
import { installationFacts } from './MilitaryInstallationCard';

const value = (facts: Fact[], label: string) => facts.find((f) => f.label === label)?.value;

describe('facts', () => {
  it('shows reported vessel facts and preserves unknown AIS values', () => {
    const facts = vesselFacts(
      vessel({
        name: ' TEST SHIP ',
        imo: '1234567',
        vesselType: 'Cargo',
        destination: 'TEST PORT',
        speedKnots: 0,
        courseDeg: 0,
        headingDeg: 360,
        lastPositionUtc: '2026-10-15T12:00:00Z',
      }),
      Date.parse('2026-10-15T12:04:30Z'),
    );
    expect(value(facts, 'Name')).toBe('TEST SHIP');
    expect(value(facts, 'MMSI')).toBe('123456789');
    expect(value(facts, 'IMO')).toBe('1234567');
    expect(value(facts, 'Destination')).toBe('TEST PORT');
    expect(value(facts, 'Speed over ground')).toBe('0 kn');
    expect(value(facts, 'Course over ground')).toBe('0°');
    expect(value(facts, 'Heading')).toBe('360°');
    expect(value(facts, 'Position time')).toBe('2026-10-15 12:00:00 UTC');
    expect(value(facts, 'Position age')).toBe('4 min');
    const unknown = vesselFacts(vessel({ speedKnots: 102.3, courseDeg: 360, headingDeg: 511 }), 0);
    for (const label of [
      'Name',
      'IMO',
      'Speed over ground',
      'Course over ground',
      'Heading',
      'Position time',
      'Position age',
    ]) {
      expect(value(unknown, label)).toBe(NOT_REPORTED);
    }
    expect(value(vesselFacts(vessel({ speedKnots: Number.NaN }), 0), 'Speed over ground')).toBe(
      NOT_REPORTED,
    );
  });

  it('describes the selected ship voyage from its track response, unknown kept unknown', () => {
    const voyage = vesselVoyageFacts({
      callSign: ' XTEST ',
      navigationalStatus: 'At anchor',
      eta: { month: 10, day: 17, hour: 8, minute: 5 },
      draughtM: 6.2,
      dimensions: { lengthM: 1_022, beamM: 12 },
    });
    expect(voyage.map((f) => [f.label, f.value])).toEqual([
      ['Call sign', 'XTEST'],
      ['Navigational status', 'At anchor'],
      ['ETA', '17 Oct 08:05 UTC'],
      ['Draught', '6.2 m'],
      ['Length × beam', `${formatCount(1_022)} × 12 m`],
    ]);
    expect(voyage[0]?.mono).toBe(true);
    for (const fact of vesselVoyageFacts(undefined)) expect(fact.value).toBe(NOT_REPORTED);
    // Impossible, "not available" or wrongly typed values from a newer server stay unknown.
    const odd = vesselVoyageFacts({
      callSign: 42 as unknown as string,
      navigationalStatus: '  ',
      eta: { month: 13, day: 1 },
      draughtM: 0,
      dimensions: { lengthM: 0, beamM: 1.5 },
    });
    for (const fact of odd) expect(fact.value).toBe(NOT_REPORTED);
    expect(vesselVoyageFacts({ draughtM: 25.6 })[3]?.value).toBe(NOT_REPORTED);
    expect(vesselVoyageFacts({ eta: 'soon' as unknown as VesselEta })[2]?.value).toBe(NOT_REPORTED);
    expect(formatVesselEta({ month: 2, day: 29, hour: 24 })).toBe('29 Feb');
    expect(formatVesselEta({ month: 12, day: 31, hour: 23 })).toBe('31 Dec 23:00 UTC');
    expect(formatVesselDimensions({ lengthM: 70 })).toBe('70 m long');
    expect(formatVesselDimensions({ beamM: 12 })).toBe('12 m beam');
  });

  it('only inspects the selected vessel with a valid position and text fields', () => {
    expect(inspect('vessels', '123456789', vessel())?.kind).toBe('vessels');
    expect(inspect('vessels', 'other', vessel())).toBeNull();
    expect(
      inspect('vessels', '123456789', vessel({ position: { lat: Number.NaN, lon: 0 } })),
    ).toBeNull();
    expect(inspect('vessels', '123456789', { ...vessel(), name: 42 })).toBeNull();
  });

  it('says "Not reported" rather than inventing values', () => {
    const facts = aircraftFacts(aircraft('abc123'));
    expect(value(facts, 'Callsign')).toBe(NOT_REPORTED);
    expect(value(facts, 'Altitude')).toBe(NOT_REPORTED);
    expect(value(facts, 'Squawk')).toBe(NOT_REPORTED);
    expect(value(facts, 'Emergency')).toBe('None');
    expect(value(aircraftFacts(aircraft('x', { onGround: true })), 'Altitude')).toBe('On ground');
    expect(value(aircraftFacts(aircraft('x', { verticalRateFpm: 1200 })), 'Vertical rate')).toMatch(
      /^\+1.200 ft\/min$/,
    );
    expect(value(aircraftFacts(aircraft('x', { emergency: 'none' })), 'Emergency')).toBe('None');
  });

  it('tones PAGER alerts and leaves the rest plain', () => {
    const quake: Earthquake = {
      id: 'us1',
      magnitude: 6.1,
      depthKm: 33.25,
      position: { lat: 0, lon: 0 },
      time: '2026-10-11T11:00:00Z',
      tsunami: false,
      alert: 'red',
      felt: 1,
    };
    const facts = earthquakeFacts(quake, Date.parse('2026-10-11T12:00:00Z'));
    expect(facts.find((f) => f.label === 'PAGER alert')).toMatchObject({
      value: 'Red',
      tone: 'alert',
    });
    expect(value(facts, 'Depth')).toBe('33.3 km');
    expect(value(facts, 'Felt')).toBe('1 report');
    expect(value(facts, 'Tsunami')).toBe('Not flagged');
    expect(value(facts, 'Time')).toBe('2026-10-11 11:00:00 UTC · 1 h ago');
    expect(value(facts, 'Review status')).toBe(NOT_REPORTED);
  });

  it('derives orbit figures and refuses non-physical ones', () => {
    expect(value(satelliteFacts(satellite(1)), 'Period')).toBe('92.9 min');
    const broken = satelliteFacts(satellite(1, { meanMotion: 0, eccentricity: 2 }));
    expect(value(broken, 'Period')).toBe(NOT_REPORTED);
    expect(value(broken, 'Perigee (approx.)')).toBe(NOT_REPORTED);
  });

  it('writes provenance with unknowns as unknown', () => {
    const facts = provenanceFacts({ sources: [], observedMs: null, fetchedMs: null, nowMs: 0 });
    expect(value(facts, 'Source')).toBe(NOT_REPORTED);
    expect(value(facts, 'Age')).toBe('Unknown');
    expect(value(facts, 'Notices')).toBeUndefined();
    const fixture = provenanceFacts({
      sources: ['AISStream'],
      observedMs: null,
      fetchedMs: null,
      nowMs: 0,
      notices: ['Recorded fixture data, not live.', ' ', 'Synthetic AIS replay.'],
    });
    expect(value(fixture, 'Notices')).toBe(
      'Recorded fixture data, not live. Synthetic AIS replay.',
    );
  });

  it('only inspects the record that matches the selection', () => {
    expect(inspect('aircraft', 'abc123', aircraft('abc123'))?.kind).toBe('aircraft');
    expect(inspect('aircraft', 'other', aircraft('abc123'))).toBeNull();
    expect(inspect('satellites', '25544', satellite(25544))?.kind).toBe('satellites');
    expect(inspect('earthquakes', 'us1', { id: 'us1' })).toBeNull();
    expect(inspect('aircraft', 'x', null)).toBeNull();
  });

  it('formats fire facts for single detections and clusters', () => {
    const single: FireCell = {
      lat: 34.05,
      lon: -118.25,
      brightness: 320.5,
      frp: 18.2,
      confidence: 'nominal',
      satellite: 'N20',
      acquiredAt: '2026-10-15T12:00:00Z',
      dayNight: 'D',
      count: 1,
      maxFrp: 18.2,
      latestAcquiredAt: '2026-10-15T12:00:00Z',
    };
    const singleFacts = fireFacts(single);
    expect(value(singleFacts, 'Radiative power')).toBe('18.2 MW');
    expect(value(singleFacts, 'Brightness')).toBe('320.5 K');
    expect(value(singleFacts, 'Confidence')).toBe('nominal');
    expect(value(singleFacts, 'Satellite')).toBe('N20');
    expect(value(singleFacts, 'Acquisition')).toBe('Day');
    expect(value(singleFacts, 'Detections')).toBeUndefined();

    const cluster: FireCell = {
      ...single,
      count: 14,
      maxFrp: 95.4,
      frp: 350.8,
      dayNight: 'N',
    };
    const clusterFacts = fireFacts(cluster);
    expect(value(clusterFacts, 'Radiative power')).toBe('350.8 MW');
    expect(value(clusterFacts, 'Detections')).toBe('14');
    expect(value(clusterFacts, 'Peak FRP')).toBe('95.4 MW');
    expect(value(clusterFacts, 'Acquisition')).toBe('Night');
  });

  it('inspects matching fire records', () => {
    const fire: FireCell = {
      lat: 34.05,
      lon: -118.25,
      brightness: 320.5,
      frp: 18.2,
      confidence: 'nominal',
      satellite: 'N20',
      acquiredAt: '2026-10-15T12:00:00Z',
      dayNight: 'D',
      count: 1,
      maxFrp: 18.2,
      latestAcquiredAt: '2026-10-15T12:00:00Z',
    };
    const id = '34.0500,-118.2500';
    expect(inspect('fires', id, fire)).toEqual({ kind: 'fires', item: fire });
    expect(inspect('fires', 'wrong-id', fire)).toBeNull();
    expect(inspect('fires', id, null)).toBeNull();
    for (const field of ['lat', 'lon', 'frp', 'count', 'maxFrp']) {
      const malformed = { ...fire } as Record<string, unknown>;
      delete malformed[field];
      expect(inspect('fires', id, malformed), field).toBeNull();
    }
    expect(inspect('fires', id, { ...fire, count: 1.5 })).toBeNull();
    expect(inspect('fires', id, { ...fire, count: 0 })).toBeNull();
    expect(inspect('fires', id, { ...fire, frp: -1 })).toBeNull();
    expect(inspect('fires', id, { ...fire, maxFrp: null })).toBeNull();
  });

  it('describes a drawn fire cell whose descriptive fields are malformed without crashing', () => {
    const fire: FireCell = {
      lat: 34.05,
      lon: -118.25,
      brightness: 320.5,
      frp: 18.2,
      confidence: 'nominal',
      satellite: 'N20',
      acquiredAt: '2026-10-15T12:00:00Z',
      dayNight: 'D',
      count: 3,
      maxFrp: 9.1,
      latestAcquiredAt: '2026-10-15T12:00:00Z',
    };
    const id = '34.0500,-118.2500';
    const malformed: Record<string, unknown> = {
      ...fire,
      brightness: null,
      confidence: 7,
      satellite: { name: 'N20' },
      dayNight: ['D'],
      acquiredAt: 1_760_000_000,
    };
    delete malformed.latestAcquiredAt;
    const inspected = inspect('fires', id, malformed);
    expect(inspected?.kind).toBe('fires');
    if (inspected?.kind !== 'fires') throw new Error('expected a fire record');
    // A normalised copy: the store's record is left as it is.
    expect(inspected.item).not.toBe(malformed);
    expect(malformed.confidence).toBe(7);
    const facts = fireFacts(inspected.item);
    expect(value(facts, 'Radiative power')).toBe('18.2 MW');
    expect(value(facts, 'Detections')).toBe('3');
    expect(value(facts, 'Peak FRP')).toBe('9.1 MW');
    for (const label of ['Brightness', 'Confidence', 'Satellite', 'Acquisition']) {
      expect(value(facts, label), label).toBe(NOT_REPORTED);
    }
    expect(inspected.item.latestAcquiredAt).toBe('');
    expect(inspected.item.acquiredAt).toBe('');
  });

  it('reads the server brightness default (0 K) and impossible values as not reported', () => {
    const fire: FireCell = {
      lat: 1,
      lon: 2,
      brightness: 0,
      frp: 0,
      confidence: '',
      satellite: '',
      acquiredAt: '2026-10-15T12:00:00Z',
      dayNight: '',
      count: 1,
      maxFrp: 0,
      latestAcquiredAt: '2026-10-15T12:00:00Z',
    };
    expect(value(fireFacts(fire), 'Brightness')).toBe(NOT_REPORTED);
    expect(value(fireFacts({ ...fire, brightness: -5 }), 'Brightness')).toBe(NOT_REPORTED);
    expect(value(fireFacts({ ...fire, brightness: Number.NaN }), 'Brightness')).toBe(NOT_REPORTED);
    expect(value(fireFacts(fire), 'Radiative power')).toBe('0.0 MW');
  });

  it('inspects exactly the fire cells the globe draws', () => {
    const base = {
      lat: 10,
      lon: 20,
      brightness: 300,
      frp: 5,
      confidence: 'n',
      satellite: 'N',
      acquiredAt: '2026-10-15T12:00:00Z',
      dayNight: 'D',
      count: 1,
      maxFrp: 5,
      latestAcquiredAt: '2026-10-15T12:00:00Z',
    };
    const variants: Record<string, unknown>[] = [
      {},
      { lat: 90 },
      { lon: -180 },
      { count: 2 ** 31 },
      { frp: 0, maxFrp: 0 },
      { brightness: null },
      { lat: 91 },
      { lon: 180.5 },
      { count: 0 },
      { count: 2.5 },
      { count: '2' },
      { frp: -0.1 },
      { frp: null },
      { maxFrp: Number.POSITIVE_INFINITY },
    ];
    for (const variant of variants) {
      const candidate = { ...base, ...variant };
      const id = `${candidate.lat.toFixed(4)},${candidate.lon.toFixed(4)}`;
      expect(inspect('fires', id, candidate) !== null, JSON.stringify(variant)).toBe(
        isValidFireCell(candidate),
      );
    }
  });

  it('formats military installation facts with operator and MGRS coordinates', () => {
    const facts = installationFacts({
      id: 'way/987654',
      name: 'Ramstein Air Base',
      kind: 'airfield',
      position: { lon: 7.6, lat: 49.43 },
      tags: {
        operator: 'United States Air Force',
        military: 'airfield',
      },
    });

    expect(value(facts, 'Facility Name')).toBe('Ramstein Air Base');
    expect(value(facts, 'Facility Type')).toBe('Military Airbase');
    expect(value(facts, 'Facility ID')).toBe('way/987654');
    expect(value(facts, 'Operator / Branch')).toBe('United States Air Force');
    expect(value(facts, 'Military Classification')).toBe('airfield');
    expect(value(facts, 'MGRS Grid')).toBeDefined();
    expect(value(facts, 'Coordinates')).toBe('49.43000°, 7.60000°');
  });

  it('trims trailing zeros and uses a real minus sign', () => {
    expect(trimmed(10, 1)).toBe('10');
    expect(trimmed(10.04, 1)).toBe('10');
    expect(trimmed(-2.5, 1)).toBe('−2.5');
  });
});
