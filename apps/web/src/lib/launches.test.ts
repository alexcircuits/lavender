import { describe, expect, it } from 'vitest';
import { CAPE_PAD, launch, padlessLaunch, WEST_PAD } from '../test/launch';
import {
  awaitingOutcome,
  findPadGroup,
  groupLaunchPads,
  hasLiftedOff,
  isPadId,
  isPlaceablePad,
  isUpcoming,
  isWellFormedLaunch,
  launchClock,
  launchPhase,
  launchTitle,
  padIdOf,
  padPickId,
  splitLaunches,
  wellFormedLaunches,
} from './launches';

/** The fixture clock the e2e journey uses: between the first and second fixture launch. */
const NOW = Date.parse('2026-06-02T12:00:00Z');

describe('launchPhase', () => {
  it('reads LL2 status names and their abbreviations, case- and space-insensitively', () => {
    expect(launchPhase('Go for Launch')).toBe('go');
    expect(launchPhase(' go ')).toBe('go');
    expect(launchPhase('To Be Confirmed')).toBe('tbc');
    expect(launchPhase('TBD')).toBe('tbd');
    expect(launchPhase('On Hold')).toBe('hold');
    expect(launchPhase('Launch in Flight')).toBe('inFlight');
    expect(launchPhase('Launch Successful')).toBe('success');
    expect(launchPhase('Success')).toBe('success');
    expect(launchPhase('Payload Deployed')).toBe('deployed');
    expect(launchPhase('Launch was a Partial Failure')).toBe('partialFailure');
    expect(launchPhase('Launch Failure')).toBe('failure');
  });

  it('keeps statuses it does not know as unknown, including inherited property names', () => {
    expect(launchPhase('Scrubbed for weather')).toBe('unknown');
    expect(launchPhase('constructor')).toBe('unknown');
    expect(launchPhase('')).toBe('unknown');
  });

  it('counts in-flight and every outcome as lifted off, nothing before', () => {
    expect(
      ['inFlight', 'success', 'deployed', 'partialFailure', 'failure'].every((p) =>
        hasLiftedOff(p as Parameters<typeof hasLiftedOff>[0]),
      ),
    ).toBe(true);
    expect(
      ['go', 'tbc', 'tbd', 'hold', 'unknown'].some((p) =>
        hasLiftedOff(p as Parameters<typeof hasLiftedOff>[0]),
      ),
    ).toBe(false);
  });
});

describe('launchTitle', () => {
  it('prefers the mission, then the name after LL2’s bar, then the whole name', () => {
    expect(launchTitle({ name: 'Falcon 9 | Mission A', mission: 'Mission A ' })).toBe('Mission A');
    expect(launchTitle({ name: 'Falcon 9 Block 5 | Starlink Group 17-40' })).toBe(
      'Starlink Group 17-40',
    );
    expect(launchTitle({ name: 'Electron test flight' })).toBe('Electron test flight');
    expect(launchTitle({ name: '  ', mission: ' ' })).toBe('Unnamed launch');
  });
});

describe('wire validation', () => {
  it('keeps well-formed launches and skips a malformed record instead of failing on it', () => {
    const good = launch('good');
    const items: unknown[] = [
      good,
      { ...launch('no-status'), status: undefined },
      { ...launch('bad-name'), name: 42 },
      { ...launch('bad-pad'), pad: { latitude: 1, longitude: 2 } },
      { ...launch('bad-location'), pad: { ...CAPE_PAD, locationName: 7 } },
      { ...launch('x'), id: '' },
      null,
      'launch',
    ];
    expect(wellFormedLaunches(items)).toEqual([good]);
    expect(isWellFormedLaunch(padlessLaunch('nopad'))).toBe(true);
    // A pad that cannot be placed is still a launch: shown in the list without a position.
    expect(isWellFormedLaunch(launch('far', { pad: { ...WEST_PAD, longitude: 200 } }))).toBe(true);
  });
});

describe('launchClock', () => {
  it('floors to the minute, so every launch surface reads one clock', () => {
    expect(launchClock(Date.parse('2026-06-02T12:00:45.900Z'))).toBe(NOW);
    expect(launchClock(NOW)).toBe(NOW);
    // A NET 30 s past the minute is still ahead 45 s past it: list and sheet agree.
    const netSoon = launch('soon', { net: '2026-06-02T12:00:30Z' });
    const clock = launchClock(Date.parse('2026-06-02T12:00:45Z'));
    expect(isUpcoming(netSoon, clock)).toBe(true);
    expect(awaitingOutcome(netSoon, clock)).toBe(false);
  });
});

describe('splitLaunches on a fixed clock', () => {
  const recentSuccess = launch('a', { status: 'Success', net: '2026-06-01T12:00:00Z' });
  const soon = launch('b', { net: '2026-06-02T18:00:00Z' });
  const later = launch('c', { status: 'Success', net: '2026-08-04T15:02:00Z' });
  const older = launch('d', { status: 'Launch Failure', net: '2026-05-20T09:00:00Z' });

  it('puts future NETs in upcoming (soonest first) and passed NETs in recent (latest first)', () => {
    const { upcoming, recent } = splitLaunches([later, older, recentSuccess, soon], NOW);
    expect(upcoming.map((l) => l.id)).toEqual(['b', 'c']);
    expect(recent.map((l) => l.id)).toEqual(['a', 'd']);
  });

  it('does not move a launch between groups because of its status', () => {
    // A synthetic "Success" with a future NET stays upcoming: the status is shown, not trusted.
    expect(isUpcoming(later, NOW)).toBe(true);
    // A NET exactly at the clock has arrived.
    expect(isUpcoming(launch('e', { net: '2026-06-02T12:00:00Z' }), NOW)).toBe(false);
  });

  it('lists launches without a NET last: upcoming unless the source reports lift-off', () => {
    const { net: _a, ...undatedGo } = launch('f');
    const { net: _b, ...undatedDone } = launch('g', { status: 'Launch Successful' });
    const { upcoming, recent } = splitLaunches([undatedGo, soon, undatedDone, older], NOW);
    expect(upcoming.map((l) => l.id)).toEqual(['b', 'f']);
    expect(recent.map((l) => l.id)).toEqual(['d', 'g']);
  });

  it('orders equal times by name, and treats an unparseable NET as unknown', () => {
    const x = launch('x', { mission: 'Bravo', net: '2026-06-03T00:00:00Z' });
    const y = launch('y', { mission: 'Alpha', net: '2026-06-03T00:00:00Z' });
    const z = launch('z', { mission: 'Charlie', net: 'not a time' });
    expect(splitLaunches([x, z, y], NOW).upcoming.map((l) => l.id)).toEqual(['y', 'x', 'z']);
  });

  it('flags a passed NET whose status is still pre-launch as awaiting an outcome', () => {
    expect(awaitingOutcome(launch('h', { net: '2026-06-01T00:00:00Z' }), NOW)).toBe(true);
    expect(awaitingOutcome(recentSuccess, NOW)).toBe(false);
    expect(awaitingOutcome(soon, NOW)).toBe(false);
    expect(
      awaitingOutcome(launch('i', { status: 'Mystery', net: '2026-06-01T00:00:00Z' }), NOW),
    ).toBe(false);
  });
});

describe('launch pads', () => {
  it('aggregates launches by pad position in first-seen order, one group per site', () => {
    const groups = groupLaunchPads([
      launch('a'),
      launch('b', { pad: WEST_PAD }),
      // 28.561945 still rounds to 28.5619: the same site to about 11 m.
      launch('c', { pad: { ...CAPE_PAD, latitude: CAPE_PAD.latitude + 0.000004 } }),
    ]);
    expect(groups.map((g) => g.id)).toEqual(['pad:28.5619,-80.5772', 'pad:34.6320,-120.6110']);
    expect(groups[0]?.launches.map((l) => l.id)).toEqual(['a', 'c']);
    expect(groups[0]).toMatchObject({
      name: 'Test Complex 40',
      locationName: 'Test Cape, FL, USA',
    });
  });

  it('joins distinct pad names at one place, and skips pads that cannot be placed', () => {
    const groups = groupLaunchPads([
      launch('a'),
      launch('b', { pad: { name: 'Other Name', latitude: 28.5619, longitude: -80.5772 } }),
      launch('c', { pad: { ...WEST_PAD, latitude: 95 } }),
      launch('d', { pad: { ...WEST_PAD, longitude: Number.NaN } }),
      padlessLaunch('e'),
    ]);
    expect(groups).toHaveLength(1);
    expect(groups[0]?.name).toBe('Test Complex 40 / Other Name');
    expect(groups[0]?.launches).toHaveLength(2);
  });

  it('validates wire pads before placing them', () => {
    expect(isPlaceablePad(CAPE_PAD)).toBe(true);
    expect(isPlaceablePad({ ...CAPE_PAD, latitude: '28.5' })).toBe(false);
    expect(isPlaceablePad({ ...CAPE_PAD, longitude: -181 })).toBe(false);
    expect(isPlaceablePad({ latitude: 1, longitude: 2 })).toBe(false);
    expect(isPlaceablePad(null)).toBe(false);
  });

  it('picks a single-launch pad as its launch, a busier pad as the pad', () => {
    const [cape, west] = groupLaunchPads([
      launch('a'),
      launch('b', { pad: WEST_PAD }),
      launch('c'),
    ]);
    if (!cape || !west) throw new Error('two pads expected');
    expect(padPickId(cape)).toBe(cape.id);
    expect(padPickId(west)).toBe('b');
    expect(isPadId(cape.id)).toBe(true);
    expect(isPadId('b')).toBe(false);
    expect(findPadGroup([cape, west], 'c')).toBe(cape);
    expect(findPadGroup([cape, west], west.id)).toBe(west);
    expect(findPadGroup([cape, west], 'pad:0.0000,0.0000')).toBeUndefined();
    expect(padIdOf(WEST_PAD)).toBe(west.id);
  });

  it('keeps one pad id either side of zero (no "-0.0000")', () => {
    expect(padIdOf({ latitude: -0.00001, longitude: -0.00004 })).toBe('pad:0.0000,0.0000');
    expect(padIdOf({ latitude: -0.00005, longitude: 0 })).toBe('pad:-0.0001,0.0000');
    const groups = groupLaunchPads([
      launch('a', { pad: { name: 'Sea platform', latitude: 0, longitude: -154 } }),
      launch('b', { pad: { name: 'Sea platform', latitude: -0.00001, longitude: -154 } }),
    ]);
    expect(groups).toHaveLength(1);
    expect(groups[0]?.launches).toHaveLength(2);
  });
});
