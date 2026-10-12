import { describe, expect, it } from 'vitest';
import { EMPTY_SUMMARY, type FeedSummary } from '../../state/stores';
import { layer } from '../../test/fakes';
import {
  magnitudeValue,
  selectedGroups,
  withGroup,
  withMilitaryOnly,
  withMinMagnitude,
} from './filters';
import { layerCondition } from './status';

const on = { enabled: true, filters: {} };
const ready = (overrides: Partial<FeedSummary> = {}): FeedSummary => ({
  ...EMPTY_SUMMARY,
  phase: 'ready',
  count: 4,
  freshness: { state: 'live' },
  coverage: { kind: 'complete' },
  ...overrides,
});

describe('layerCondition', () => {
  it('reads availability before anything else', () => {
    expect(
      layerCondition(
        layer('aircraft', { availability: { state: 'unconfigured', missing: ['A', 'B'] } }),
        on,
        ready(),
      ),
    ).toMatchObject({ kind: 'unconfigured', pill: 'not-configured', reason: 'Needs A, B' });
    expect(
      layerCondition(
        layer('aircraft', { availability: { state: 'disabledByLicence' } }),
        on,
        ready(),
      ),
    ).toMatchObject({ kind: 'licence', pill: 'licence' });
  });

  it('follows the subscription from off to connecting to live', () => {
    const l = layer('earthquakes');
    expect(layerCondition(l, undefined, undefined).kind).toBe('off');
    expect(layerCondition(l, on, undefined).kind).toBe('connecting');
    expect(layerCondition(l, on, EMPTY_SUMMARY).kind).toBe('connecting');
    expect(layerCondition(l, on, ready())).toMatchObject({ kind: 'live', pill: null });
  });

  it('never shows stale, degraded or failed data as live', () => {
    const l = layer('earthquakes');
    expect(
      layerCondition(l, on, ready({ freshness: { state: 'stale', ageS: 300 } })),
    ).toMatchObject({ kind: 'stale', pill: 'stale', pillDetail: '5 min' });
    expect(
      layerCondition(l, on, ready({ freshness: { state: 'degraded', reason: 'Slow', ageS: 9 } })),
    ).toMatchObject({ kind: 'degraded', reason: 'Slow' });
    expect(
      layerCondition(l, on, ready({ freshness: { state: 'unavailable', reason: 'Down' } })),
    ).toMatchObject({ kind: 'unavailable', reason: 'Down' });
    expect(
      layerCondition(
        l,
        on,
        ready({ phase: 'error', error: { code: 'internal', message: 'Broke', retryable: true } }),
      ),
    ).toMatchObject({ kind: 'unavailable', pill: 'unavailable', reason: 'Broke' });
    expect(
      layerCondition(l, on, ready({ coverage: { kind: 'partial', reason: '2 regions pending' } })),
    ).toMatchObject({ kind: 'live', reason: '2 regions pending' });
  });
});

describe('filters', () => {
  it('stores a minimum magnitude only when it is not "All"', () => {
    expect(withMinMagnitude({}, '4.5')).toEqual({ minMagnitude: 4.5 });
    expect(withMinMagnitude({ minMagnitude: 4.5, militaryOnly: true }, 'all')).toEqual({
      militaryOnly: true,
    });
    expect(magnitudeValue({ minMagnitude: 6 })).toBe('6');
    expect(magnitudeValue({ minMagnitude: 3.3 })).toBe('all');
  });

  it('stores military-only only when on', () => {
    expect(withMilitaryOnly({}, true)).toEqual({ militaryOnly: true });
    expect(withMilitaryOnly({ militaryOnly: true }, false)).toEqual({});
  });

  it('treats no groups as all groups and drops the filter when all are back', () => {
    expect(selectedGroups({})).toHaveLength(6);
    const without = withGroup({}, 'science', false);
    expect(without.groups).not.toContain('science');
    expect(withGroup(without, 'science', true)).toEqual({});
    expect(withGroup({ groups: ['stations', 'custom-x'] }, 'visual', true).groups).toEqual([
      'stations',
      'visual',
      'custom-x',
    ]);
  });
});
