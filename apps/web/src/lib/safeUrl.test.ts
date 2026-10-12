import { describe, expect, it } from 'vitest';
import { safeHttpsUrl, usgsEventUrl } from './safeUrl';
import { formatUtcDateTime, formatUtcTime, parseTime, secondsBetween } from './time';

describe('safeHttpsUrl', () => {
  it('accepts absolute https links', () => {
    expect(safeHttpsUrl('https://earthquake.usgs.gov/')).toBe('https://earthquake.usgs.gov/');
  });

  it('rejects everything else', () => {
    for (const raw of [
      'http://example.com',
      'javascript:alert(1)',
      'data:text/html,hi',
      '//example.com',
      '/relative',
      'https://user:pass@example.com',
      '',
      null,
      undefined,
    ]) {
      expect(safeHttpsUrl(raw), String(raw)).toBeNull();
    }
  });
});

describe('usgsEventUrl', () => {
  it('accepts usgs.gov and its subdomains over https', () => {
    const url = 'https://earthquake.usgs.gov/earthquakes/eventpage/us7000abcd';
    expect(usgsEventUrl(url)).toBe(url);
    expect(usgsEventUrl('https://usgs.gov/x')).toBe('https://usgs.gov/x');
  });

  it('rejects look-alike hosts and plain http', () => {
    expect(usgsEventUrl('https://usgs.gov.example.com/x')).toBeNull();
    expect(usgsEventUrl('https://evilusgs.gov/x')).toBeNull();
    expect(usgsEventUrl('http://earthquake.usgs.gov/x')).toBeNull();
  });
});

describe('time', () => {
  it('parses ISO times and refuses garbage', () => {
    expect(parseTime('2026-10-11T12:00:00Z')).toBe(Date.UTC(2026, 9, 11, 12));
    expect(parseTime('soon')).toBeNull();
    expect(parseTime(null)).toBeNull();
  });

  it('formats UTC and never reports negative ages', () => {
    const ms = Date.UTC(2026, 9, 11, 14, 2, 11);
    expect(formatUtcTime(ms)).toBe('14:02:11 UTC');
    expect(formatUtcDateTime(ms)).toBe('2026-10-11 14:02:11 UTC');
    expect(secondsBetween(ms, ms + 4_500)).toBe(4);
    expect(secondsBetween(ms + 4_500, ms)).toBe(0);
  });
});
