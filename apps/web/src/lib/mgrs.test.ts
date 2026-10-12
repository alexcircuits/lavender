import { describe, expect, it } from 'vitest';
import { fromMgrs, fromUtm, spaceMgrs, toMgrs, toUtm, utmZone } from './mgrs';

/** Great-circle distance in metres (spherical; enough for round-trip tolerances). */
function distanceM(a: { lat: number; lon: number }, b: { lat: number; lon: number }): number {
  const r = 6_371_008.8;
  const rad = Math.PI / 180;
  const dLat = (b.lat - a.lat) * rad;
  const dLon = (b.lon - a.lon) * rad;
  const h =
    Math.sin(dLat / 2) ** 2 +
    Math.cos(a.lat * rad) * Math.cos(b.lat * rad) * Math.sin(dLon / 2) ** 2;
  return 2 * r * Math.asin(Math.sqrt(h));
}

/** Deterministic pseudo-random points (no flaky property tests). */
function* samplePoints(count: number): Generator<{ lat: number; lon: number }> {
  let seed = 42;
  const next = () => {
    seed = (seed * 1_103_515_245 + 12_345) % 2 ** 31;
    return seed / 2 ** 31;
  };
  for (let i = 0; i < count; i += 1) {
    yield { lat: -79.9 + next() * 163.8, lon: -179.9 + next() * 359.8 };
  }
}

describe('toMgrs', () => {
  // Expected values computed independently with Krüger's n-series (as in lavender-geo).
  it('formats the White House, the origin and Vienna', () => {
    expect(toMgrs(38.897676, -77.036548)).toBe('18SUJ2339007393');
    expect(toMgrs(0, 0)).toBe('31NAA6602100000');
    expect(toMgrs(48.24949, 16.4145, 5)).toBe('33UXP0500444997');
    expect(toMgrs(48.24949, 16.4145, 1)).toBe('33UXP04');
  });

  it('clamps precision to 0–5 digits; 0 is the bare 100 km square', () => {
    expect(toMgrs(48.24949, 16.4145, 9)).toBe('33UXP0500444997');
    expect(toMgrs(48.24949, 16.4145, 3)).toBe('33UXP050449');
    expect(toMgrs(48.24949, 16.4145, 0)).toBe('33UXP');
    expect(toMgrs(38.897676, -77.036548, 0)).toBe('18SUJ');
    expect(toMgrs(48.24949, 16.4145, -2)).toBe('33UXP');
    expect(toMgrs(48.24949, 16.4145, Number.NaN)).toBe('33UXP0500444997');
  });

  it('is null outside the MGRS bands and for invalid positions', () => {
    expect(toMgrs(85, 0)).toBeNull();
    expect(toMgrs(-80.5, 0)).toBeNull();
    expect(toMgrs(Number.NaN, 0)).toBeNull();
    expect(toMgrs(10, 181)).toBeNull();
  });

  it('labels the southern hemisphere', () => {
    expect(toMgrs(-33.8568, 151.2153)?.startsWith('56H')).toBe(true);
  });
});

describe('utmZone', () => {
  it('applies the Norway and Svalbard exceptions', () => {
    expect(utmZone(60, 5)).toBe(32);
    expect(utmZone(78, 15)).toBe(33);
    expect(utmZone(78, 25)).toBe(35);
    expect(utmZone(78, 35)).toBe(37);
    expect(utmZone(78, 5)).toBe(31);
  });

  it('numbers zones from the antimeridian', () => {
    expect(utmZone(0, -180)).toBe(1);
    expect(utmZone(0, 180)).toBe(60);
    expect(utmZone(0, 0)).toBe(31);
  });
});

describe('UTM', () => {
  it('matches the reference projection to the centimetre', () => {
    const utm = toUtm(38.897676, -77.036548);
    expect(utm?.zone).toBe(18);
    expect(utm?.easting).toBeCloseTo(323_390.07, 1);
    expect(utm?.northing).toBeCloseTo(4_307_393.06, 1);
  });

  it('round-trips through the inverse', () => {
    for (const p of samplePoints(200)) {
      const utm = toUtm(p.lat, p.lon);
      const back = utm ? fromUtm(utm) : null;
      expect(back && distanceM(back, p)).toBeLessThan(0.01);
    }
  });
});

describe('fromMgrs', () => {
  it('returns the centre of the referenced square', () => {
    const back = fromMgrs('18SUJ2339007393');
    expect(back && distanceM(back, { lat: 38.897676, lon: -77.036548 })).toBeLessThan(1.5);
  });

  it('accepts spaces and any case', () => {
    const back = fromMgrs('18s uj 23390 07393');
    expect(back && distanceM(back, { lat: 38.897676, lon: -77.036548 })).toBeLessThan(1.5);
  });

  it('decodes coarse and square-only references', () => {
    const tenKm = fromMgrs('33UXP04');
    expect(tenKm && distanceM(tenKm, { lat: 48.24949, lon: 16.4145 })).toBeLessThan(10_000);
    const square = fromMgrs('33UXP');
    expect(square && distanceM(square, { lat: 48.24949, lon: 16.4145 })).toBeLessThan(100_000);
  });

  it('round-trips within a metre at 1 m precision, everywhere in the bands', () => {
    for (const p of samplePoints(300)) {
      const mgrs = toMgrs(p.lat, p.lon, 5);
      const back = mgrs ? fromMgrs(mgrs) : null;
      expect(back && distanceM(back, p), `${p.lat},${p.lon} → ${mgrs}`).toBeLessThan(1.5);
    }
  });

  it('round-trips coarse references next to band edges', () => {
    for (const lat of [47.9999, 48.0001, -0.0001, 0.0001, 55.9999, 72.0001, 83.9]) {
      for (const precision of [1, 2, 3]) {
        const mgrs = toMgrs(lat, 16.4, precision);
        expect(mgrs && fromMgrs(mgrs), `${lat} @ ${precision}: ${mgrs}`).not.toBeNull();
      }
    }
  });

  it('rejects a square outside its stated band, as mgrs.rs does', () => {
    // "18SUL…" decodes to about 40.70° N, which is band T, not S.
    expect(fromMgrs('18SUL2339007393')).toBeNull();
    const right = fromMgrs('18TUL2339007393');
    expect(right?.lat).toBeCloseTo(40.7, 1);
  });

  it('lets coarse squares straddle a band edge, but not fine ones', () => {
    // 39.95° N is in band S; its 100 km square reaches north of 40°, into band T.
    const coarse = toMgrs(39.95, -75, 0) ?? '';
    expect(coarse.startsWith('18S')).toBe(true);
    const a = fromMgrs(coarse);
    const b = fromMgrs(coarse.replace('18S', '18T'));
    expect(a && b && distanceM(a, b)).toBeLessThan(1);
    const fine = (toMgrs(39.95, -75, 5) ?? '').replace('18S', '18T');
    expect(fromMgrs(fine)).toBeNull();
  });

  it('rejects malformed references and impossible bands', () => {
    for (const bad of [
      'hello',
      '18SUJ233', // odd digit count
      '61SUJ', // zone out of range
      '0SUJ',
      '18SIJ', // I is never used
      '18SUJ23390073931234', // too many digits
      '18AUJ2339007393', // A is not a band
      '18TUJ2339007393', // decodes in band S, not T
      '18SAJ2339007393', // A is not a column letter in zone 18
    ]) {
      expect(fromMgrs(bad), bad).toBeNull();
    }
  });
});

describe('spaceMgrs', () => {
  it('groups square, easting and northing', () => {
    expect(spaceMgrs('18SUJ2339007393')).toBe('18SUJ 23390 07393');
    expect(spaceMgrs('33UXP04')).toBe('33UXP 0 4');
    expect(spaceMgrs('33UXP')).toBe('33UXP');
  });
});
