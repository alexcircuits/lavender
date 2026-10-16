import { describe, expect, it } from 'vitest';
import { groupLaunchPads } from '../../lib/launches';
import { CAPE_PAD, launch, padlessLaunch, WEST_PAD } from '../../test/launch';
import { type Fact, inspect, NOT_REPORTED } from './facts';
import { launchFacts, padFacts } from './launchFacts';

const NOW = Date.parse('2026-06-02T12:00:00Z');

const value = (facts: readonly Fact[], label: string) =>
  facts.find((fact) => fact.label === label)?.value;

describe('launch facts', () => {
  const full = launch('l1', {
    name: 'Test Rocket | Mission One',
    status: 'Go for Launch',
    net: '2026-06-02T18:00:00Z',
    windowStart: '2026-06-02T17:45:00Z',
    windowEnd: '2026-06-02T21:45:00Z',
    orbit: 'Low Earth Orbit',
    launchDesignator: '2026-101',
  });

  it('writes every Launch field, with UTC and relative times on one clock', () => {
    const facts = launchFacts(full, NOW);
    expect(facts.find((f) => f.label === 'Status')).toEqual({
      label: 'Status',
      value: 'Go',
      tone: 'good',
    });
    expect(value(facts, 'No earlier than (NET)')).toBe('2026-06-02 18:00:00 UTC · in 6 h');
    expect(value(facts, 'Window')).toBe('2026-06-02 17:45–21:45 UTC (4 h)');
    expect(value(facts, 'Provider')).toBe('Test Provider');
    expect(value(facts, 'Rocket')).toBe('Test Rocket Block 1');
    expect(value(facts, 'Mission')).toBe('Mission l1');
    expect(value(facts, 'Orbit')).toBe('Low Earth Orbit');
    expect(value(facts, 'Launch name')).toBe('Test Rocket | Mission One');
    expect(value(facts, 'Launch designator')).toBe('2026-101');
    expect(value(facts, 'Pad')).toBe('Test Complex 40');
    expect(value(facts, 'Location')).toBe('Test Cape, FL, USA');
    expect(value(facts, 'Position')).toBe('28.5619° N, 80.5772° W');
    expect(value(facts, 'MGRS')).toMatch(/^17R/);
    expect(facts.find((f) => f.label === 'Launch ID')).toMatchObject({ value: 'l1', mono: true });
  });

  it('says "Not reported" for what the source left out, never a guess', () => {
    const { net: _n, provider: _p, rocket: _r, mission: _m, ...bare } = padlessLaunch('l2');
    const facts = launchFacts(bare, NOW);
    for (const label of [
      'No earlier than (NET)',
      'Window',
      'Provider',
      'Rocket',
      'Mission',
      'Orbit',
      'Launch designator',
      'Pad',
      'Location',
    ]) {
      expect(value(facts, label), label).toBe(NOT_REPORTED);
    }
    expect(value(facts, 'Position')).toBeUndefined();
  });

  it('flags a passed NET whose status is still pre-launch, and half-open windows', () => {
    const late = launch('l3', {
      net: '2026-06-01T09:00:00Z',
      windowStart: '2026-06-01T09:00:00Z',
    });
    const facts = launchFacts(late, NOW);
    expect(facts.find((f) => f.label === 'Status')).toEqual({
      label: 'Status',
      value: 'Go · no outcome reported yet',
      tone: 'warning',
    });
    expect(value(facts, 'Window')).toBe('Opens 2026-06-01 09:00 UTC');
    expect(
      value(launchFacts(launch('l4', { windowEnd: '2026-06-03T01:00:00Z' }), NOW), 'Window'),
    ).toBe('Closes 2026-06-03 01:00 UTC');
  });
});

describe('pad facts', () => {
  it('summarise a pad’s launches on the clock: counts, next and latest', () => {
    const [cape] = groupLaunchPads([
      launch('done', { status: 'Success', mission: 'Done', net: '2026-06-01T12:00:00Z' }),
      launch('soon', { mission: 'Soon', net: '2026-06-02T18:00:00Z' }),
      launch('west', { pad: WEST_PAD }),
      launch('later', { mission: 'Later', net: '2026-07-01T00:00:00Z' }),
    ]);
    if (!cape) throw new Error('pad expected');
    const facts = padFacts(cape, NOW);
    expect(value(facts, 'Pad')).toBe(CAPE_PAD.name);
    expect(value(facts, 'Location')).toBe(CAPE_PAD.locationName);
    expect(value(facts, 'Launches')).toBe('3 · 2 upcoming, 1 recent');
    expect(value(facts, 'Next launch')).toBe('Soon · in 6 h');
    expect(value(facts, 'Latest launch')).toBe('Done · 24 h ago');
    expect(value(facts, 'Position')).toBe('28.5619° N, 80.5772° W');
  });

  it('says when a pad has nothing upcoming or recent', () => {
    const [pad] = groupLaunchPads([launch('a', { net: '2026-06-10T00:00:00Z' })]);
    if (!pad) throw new Error('pad expected');
    expect(value(padFacts(pad, NOW), 'Latest launch')).toBe('None reported');
  });
});

describe('inspect launches', () => {
  it('accepts the selected launch record and rejects another or a malformed one', () => {
    const record = launch('l1', { matchedNoradIds: [25544] });
    expect(inspect('launches', 'l1', record)).toEqual({ kind: 'launches', item: record });
    expect(inspect('launches', 'other', record)).toBeNull();
    expect(inspect('launches', 'l1', { ...record, payloadNames: [1] })).toBeNull();
    expect(inspect('launches', 'l1', { ...record, matchedNoradIds: ['25544'] })).toBeNull();
    expect(inspect('launches', 'l1', { ...record, net: 5 })).toBeNull();
    expect(inspect('launches', 'l1', { ...record, pad: 'Cape' })).toBeNull();
    expect(inspect('launches', 'l1', null)).toBeNull();
  });
});
