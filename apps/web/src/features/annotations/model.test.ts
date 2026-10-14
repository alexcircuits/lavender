import { describe, expect, it } from 'vitest';
import { pin } from '../../test/annotation';
import { MAX_ANNOTATIONS, MAX_VERTICES, validateAnnotations } from './model';

describe('persisted annotations', () => {
  it('validates and copies resolved geometry without importing arbitrary fields', () => {
    expect(validateAnnotations([pin])).toEqual([pin]);
    expect(validateAnnotations([pin])[0]).not.toBe(pin);
  });
  it('bounds the UTF-8 storage envelope as well as individual fields', () => {
    const lines = Array.from({ length: 24 }, (_, index) => ({
      ...pin,
      id: `line-${index}`,
      kind: 'line',
      label: '界'.repeat(120),
      note: '界'.repeat(500),
      points: Array.from({ length: 32 }, (_, i) => ({
        lat: 0.123456789012345,
        lon: -160 + i * 10 + 0.123456789012345,
      })),
    }));
    expect(() => validateAnnotations(lines)).toThrow(/storage is full/);
  });
  it.for<unknown[]>([
    [{ ...pin, extra: true }],
    [{ ...pin, points: [{ lat: 91, lon: 0 }] }],
    [{ ...pin, points: [{ lat: 0, lon: Number.NaN }] }],
    [{ ...pin, points: [{ lat: 0, lon: 0, altM: 1 }] }],
    [{ ...pin, kind: 'url' }],
    [{ ...pin, approximate: false }],
    [{ ...pin, label: 'x'.repeat(121) }],
    [{ ...pin, note: 'x'.repeat(501) }],
    [{ ...pin, label: 'bad\u0000' }],
    [{ ...pin, createdAt: '2026-02-30T00:00:00.000Z' }],
    [pin, pin],
    Array.from({ length: MAX_ANNOTATIONS + 1 }, (_, i) => ({ ...pin, id: `mark-${i}` })),
    [
      {
        ...pin,
        kind: 'line',
        points: Array.from({ length: MAX_VERTICES + 1 }, (_, i) => ({ lat: 0, lon: i })),
      },
    ],
  ])('rejects invalid, ambiguous and excessive marks', (value) =>
    expect(() => validateAnnotations(value)).toThrow(),
  );
});
