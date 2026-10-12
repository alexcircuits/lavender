import { describe, expect, it } from 'vitest';
import { bboxFromRectangle, lonSpan, toViewState, wrapLongitude } from './view';

const rad = (deg: number) => (deg * Math.PI) / 180;
const rect = (west: number, south: number, east: number, north: number) => ({
  west: rad(west),
  south: rad(south),
  east: rad(east),
  north: rad(north),
});

describe('view rectangle → bbox', () => {
  it('converts a regional view to degrees', () => {
    expect(bboxFromRectangle(rect(-10, 35, 20, 60))).toEqual({
      west: -10,
      south: 35,
      east: 20,
      north: 60,
    });
  });

  it('is null when the camera sees no ellipsoid', () => {
    expect(bboxFromRectangle(undefined)).toBeNull();
  });

  it("is null for Cesium's whole-globe rectangle (horizon or space in view)", () => {
    expect(bboxFromRectangle(rect(-180, -90, 180, 90))).toBeNull();
  });

  it('is null when the view is roughly a hemisphere or wider', () => {
    expect(bboxFromRectangle(rect(-100, -10, 85, 10))).toBeNull(); // 185° of longitude
    expect(bboxFromRectangle(rect(-20, -50, 20, 45))).toBeNull(); // 95° of latitude
    expect(bboxFromRectangle(rect(-89, -40, 89, 40))).not.toBeNull(); // 178° × 80°
  });

  it('keeps west > east for a view across the antimeridian', () => {
    expect(bboxFromRectangle(rect(170, -20, -170, 10))).toEqual({
      west: 170,
      south: -20,
      east: -170,
      north: 10,
    });
  });

  it('is null for a wide view across the antimeridian', () => {
    expect(bboxFromRectangle(rect(60, -20, -60, 10))).toBeNull(); // 240° of longitude
  });

  it('wraps out-of-range longitudes and rounds away float noise', () => {
    expect(bboxFromRectangle(rect(175, 0, 185, 5))).toEqual({
      west: 175,
      south: 0,
      east: -175,
      north: 5,
    });
    const box = bboxFromRectangle(rect(1.234567891, 2, 3, 4));
    expect(box?.west).toBe(1.2346);
  });

  it('rejects non-finite or degenerate rectangles', () => {
    expect(bboxFromRectangle(rect(Number.NaN, 0, 1, 1))).toBeNull();
    expect(bboxFromRectangle(rect(10, 5, 10, 6))).toBeNull();
  });
});

describe('ViewState', () => {
  it('carries centre and height', () => {
    const view = toViewState(rect(0, 0, 10, 10), { lat: 5.000001, lon: 365 }, 1_200_000);
    expect(view).toEqual({
      bbox: { west: 0, south: 0, east: 10, north: 10 },
      centre: { lat: 5, lon: 5 },
      heightM: 1_200_000,
    });
  });

  it('allows a null centre (sky at the canvas centre) and sanitises height', () => {
    expect(toViewState(undefined, null, Number.NaN)).toEqual({
      bbox: null,
      centre: null,
      heightM: 0,
    });
  });
});

describe('longitude helpers', () => {
  it('wraps into [-180, 180]', () => {
    expect(wrapLongitude(190)).toBe(-170);
    expect(wrapLongitude(-190)).toBe(170);
    expect(wrapLongitude(540)).toBe(180);
    expect(wrapLongitude(-45)).toBe(-45);
  });

  it('measures spans across the antimeridian', () => {
    expect(lonSpan(-10, 20)).toBe(30);
    expect(lonSpan(170, -170)).toBe(20);
  });
});
