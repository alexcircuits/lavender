import { describe, expect, it } from 'vitest';
import { type CoordinateFormat, formatDecimal, formatDms, parseCoordinates } from './coordinates';

function expectPoint(input: string, lat: number, lon: number, format: CoordinateFormat) {
  const parsed = parseCoordinates(input);
  expect(parsed, input).not.toBeNull();
  expect(parsed?.format, input).toBe(format);
  expect(parsed?.position.lat, input).toBeCloseTo(lat, 4);
  expect(parsed?.position.lon, input).toBeCloseTo(lon, 4);
}

const HELSINKI_DMS = [60 + 10 / 60 + 12 / 3600, 24 + 56 / 60 + 18 / 3600] as const;

describe('parseCoordinates', () => {
  it('reads decimal pairs as latitude, longitude', () => {
    expectPoint('60.17, 24.94', 60.17, 24.94, 'decimal');
    expectPoint('60.1699, 24.9384', 60.1699, 24.9384, 'decimal');
    expectPoint('-33.8568 151.2153', -33.8568, 151.2153, 'decimal');
    expectPoint('  60.17;24.94  ', 60.17, 24.94, 'decimal');
  });

  it('reads hemisphere letters before or after the numbers', () => {
    expectPoint('60.17N 24.94E', 60.17, 24.94, 'decimal');
    expectPoint('33.86 s, 151.21 e', -33.86, 151.21, 'decimal');
    expectPoint('N 60.17 W 24.94', 60.17, -24.94, 'decimal');
    expectPoint('24.94E 60.17N', 60.17, 24.94, 'decimal');
  });

  it('reads degrees, minutes and seconds', () => {
    expectPoint(`60°10'12"N 24°56'18"E`, HELSINKI_DMS[0], HELSINKI_DMS[1], 'dms');
    expectPoint('60°10′12″ N, 24°56′18″ E', HELSINKI_DMS[0], HELSINKI_DMS[1], 'dms');
    expectPoint('N 60 10 12 E 24 56 18', HELSINKI_DMS[0], HELSINKI_DMS[1], 'dms');
    expectPoint("E24°56.3' N60°10.2'", 60.17, 24.938333, 'dms');
    expectPoint('60 10 12 24 56 18', HELSINKI_DMS[0], HELSINKI_DMS[1], 'dms');
    expectPoint(
      '-33 51 24, 151 12 55',
      -(33 + 51 / 60 + 24 / 3600),
      151 + 12 / 60 + 55 / 3600,
      'dms',
    );
  });

  it('reads MGRS without a network call', () => {
    const parsed = parseCoordinates('18S UJ 23390 07393');
    expect(parsed?.format).toBe('mgrs');
    expect(parsed?.position.lat).toBeCloseTo(38.897676, 4);
    expect(parsed?.position.lon).toBeCloseTo(-77.036548, 4);
    expect(parseCoordinates('33uxp04')?.format).toBe('mgrs');
  });

  it('rejects things that are not coordinates', () => {
    for (const text of [
      'Helsinki',
      '',
      '   ',
      '91, 0',
      '0, 181',
      '60 70 00 N 24 E',
      '1 2 3',
      'N 60 N 24',
      'SAS123',
      'N12345',
      '60.1.2, 3',
      '- 5, 3',
      '60.5 10 12 24 56 18',
      'x'.repeat(65),
    ]) {
      expect(parseCoordinates(text), text).toBeNull();
    }
  });
});

describe('formatting', () => {
  it('writes decimal degrees with hemispheres and four decimals', () => {
    expect(formatDecimal({ lat: 60.17, lon: 24.94 })).toBe('60.1700° N, 24.9400° E');
    expect(formatDecimal({ lat: 60.17, lon: -24.94 })).toBe('60.1700° N, 24.9400° W');
    expect(formatDecimal({ lat: -33.8568, lon: 151.2153 })).toBe('33.8568° S, 151.2153° E');
  });

  it('writes degrees, minutes and seconds', () => {
    expect(formatDms({ lat: 60.17, lon: 24.9384 })).toBe('60°10′12″ N 24°56′18″ E');
  });

  it('round-trips its own labels', () => {
    const point = { lat: -12.3456, lon: 98.7654 };
    expect(parseCoordinates(formatDecimal(point))?.position.lat).toBeCloseTo(point.lat, 4);
    expect(parseCoordinates(formatDms(point))?.position.lon).toBeCloseTo(point.lon, 3);
  });
});
