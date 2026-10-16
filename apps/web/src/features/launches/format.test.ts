import { describe, expect, it } from 'vitest';
import {
  CheckCircleIcon,
  CheckIcon,
  PauseIcon,
  QuestionIcon,
  RocketLaunchIcon,
  XCircleIcon,
} from '../../design';
import { launchPhase } from '../../lib/launches';
import { formatRelative, formatUtcMinute, formatWindow, launchStatusSpec } from './format';

const NOW = Date.parse('2026-06-02T12:00:00Z');
const at = (iso: string) => Date.parse(iso);

describe('formatRelative on a fixed clock', () => {
  it('writes future times as "in …" and past ones as "… ago"', () => {
    expect(formatRelative(at('2026-06-02T18:00:00Z'), NOW)).toBe('in 6 h');
    expect(formatRelative(at('2026-06-01T12:00:00Z'), NOW)).toBe('24 h ago');
    expect(formatRelative(at('2026-08-04T15:02:00Z'), NOW)).toBe('in 63 d');
    expect(formatRelative(at('2026-06-02T12:45:00Z'), NOW)).toBe('in 45 min');
    expect(formatRelative(at('2026-06-02T13:30:00Z'), NOW)).toBe('in 1 h 30 min');
  });

  it('reads anything within a minute either way as now', () => {
    expect(formatRelative(NOW, NOW)).toBe('now');
    expect(formatRelative(NOW + 59_000, NOW)).toBe('now');
    expect(formatRelative(NOW - 59_000, NOW)).toBe('now');
    expect(formatRelative(NOW + 60_000, NOW)).toBe('in 1 min');
  });
});

describe('UTC launch times', () => {
  it('writes a NET to the minute', () => {
    expect(formatUtcMinute(at('2026-06-02T18:00:59Z'))).toBe('2026-06-02 18:00 UTC');
  });

  it('writes windows on one day, across midnight, instantaneous and inverted', () => {
    expect(formatWindow(at('2026-06-02T17:45:00Z'), at('2026-06-02T21:45:00Z'))).toBe(
      '2026-06-02 17:45–21:45 UTC (4 h)',
    );
    expect(formatWindow(at('2026-06-02T23:30:00Z'), at('2026-06-03T00:15:00Z'))).toBe(
      '2026-06-02 23:30–2026-06-03 00:15 UTC (45 min)',
    );
    expect(formatWindow(at('2026-08-04T15:02:00Z'), at('2026-08-04T15:02:00Z'))).toBe(
      '2026-08-04 15:02 UTC (instantaneous)',
    );
    expect(formatWindow(at('2026-06-03T00:00:00Z'), at('2026-06-02T00:00:00Z'))).toBe(
      '2026-06-03 00:00 UTC – 2026-06-02 00:00 UTC',
    );
  });
});

describe('launch status words', () => {
  const spec = (status: string) => launchStatusSpec(launchPhase(status), status);

  it('is an icon and a word in a status tone for every LL2 status', () => {
    expect(spec('Go for Launch')).toEqual({ word: 'Go', tone: 'good', icon: CheckIcon });
    expect(spec('To Be Confirmed')).toMatchObject({ word: 'To be confirmed', tone: 'neutral' });
    expect(spec('TBD')).toMatchObject({ word: 'To be determined', icon: QuestionIcon });
    expect(spec('On Hold')).toEqual({ word: 'On hold', tone: 'warning', icon: PauseIcon });
    expect(spec('Launch in Flight')).toMatchObject({ word: 'In flight', icon: RocketLaunchIcon });
    expect(spec('Success')).toEqual({ word: 'Success', tone: 'good', icon: CheckCircleIcon });
    expect(spec('Payload Deployed')).toMatchObject({ word: 'Payload deployed', tone: 'good' });
    expect(spec('Launch was a Partial Failure')).toMatchObject({ tone: 'warning' });
    expect(spec('Launch Failure')).toEqual({ word: 'Failure', tone: 'alert', icon: XCircleIcon });
  });

  it('keeps the source’s own words for a status it does not know', () => {
    expect(spec('Scrubbed')).toEqual({ word: 'Scrubbed', tone: 'neutral', icon: QuestionIcon });
    expect(spec('  ')).toMatchObject({ word: 'Unknown' });
  });
});
