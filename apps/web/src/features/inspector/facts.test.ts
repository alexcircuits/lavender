import { describe, expect, it } from 'vitest';
import { aircraft, satellite } from '../../app/testing';
import type { Earthquake } from '../../contracts';
import {
  aircraftFacts,
  earthquakeFacts,
  type Fact,
  inspect,
  NOT_REPORTED,
  provenanceFacts,
  satelliteFacts,
  trimmed,
} from './facts';

const value = (facts: Fact[], label: string) => facts.find((f) => f.label === label)?.value;

describe('facts', () => {
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
  });

  it('only inspects the record that matches the selection', () => {
    expect(inspect('aircraft', 'abc123', aircraft('abc123'))?.kind).toBe('aircraft');
    expect(inspect('aircraft', 'other', aircraft('abc123'))).toBeNull();
    expect(inspect('satellites', '25544', satellite(25544))?.kind).toBe('satellites');
    expect(inspect('earthquakes', 'us1', { id: 'us1' })).toBeNull();
    expect(inspect('aircraft', 'x', null)).toBeNull();
  });

  it('trims trailing zeros and uses a real minus sign', () => {
    expect(trimmed(10, 1)).toBe('10');
    expect(trimmed(10.04, 1)).toBe('10');
    expect(trimmed(-2.5, 1)).toBe('−2.5');
  });
});
