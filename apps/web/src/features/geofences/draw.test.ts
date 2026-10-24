import { describe, expect, it } from 'vitest';
import type { GeoPoint } from '../../contracts';
import {
  canCloseGeofence,
  geofenceDrawReducer,
  geofencePoints,
  geofencePreviewPoints,
  rectanglePoints,
} from './draw';

const p = (lat: number, lon: number): GeoPoint => ({ lat, lon });

describe('geofence draw reducer', () => {
  it('collects polygon vertices and closes on demand', () => {
    let s = geofenceDrawReducer(null, { type: 'start', mode: 'polygon' });
    s = geofenceDrawReducer(s, { type: 'point', point: p(10, 10), at: 1_000 });
    s = geofenceDrawReducer(s, { type: 'point', point: p(10, 11), at: 1_100 });
    expect(canCloseGeofence(s)).toBe(false);
    s = geofenceDrawReducer(s, { type: 'point', point: p(11, 11), at: 1_200 });
    expect(canCloseGeofence(s)).toBe(true);
    expect(geofencePoints(s as never)).toBeNull(); // not closed yet
    s = geofenceDrawReducer(s, { type: 'close' });
    expect(s?.closed).toBe(true);
    expect(geofencePoints(s as never)).toHaveLength(3);
  });

  it('treats a second click on the last vertex within the double-click window as close', () => {
    let s = geofenceDrawReducer(null, { type: 'start', mode: 'polygon' });
    s = geofenceDrawReducer(s, { type: 'point', point: p(0, 0), at: 1_000 });
    s = geofenceDrawReducer(s, { type: 'point', point: p(1, 1), at: 1_050 });
    s = geofenceDrawReducer(s, { type: 'point', point: p(2, 2), at: 1_100 });
    // Same spot as (2, 2), well inside 400 ms and 30 m: closes.
    const closed = geofenceDrawReducer(s, { type: 'point', point: p(2, 2), at: 1_200 });
    expect(closed?.closed).toBe(true);
    // The closing click is not stored as a fourth vertex.
    expect(geofencePoints(closed as never)).toHaveLength(3);
  });

  it('does not double-close when the repeat click is slow or far away', () => {
    let s = geofenceDrawReducer(null, { type: 'start', mode: 'polygon' });
    s = geofenceDrawReducer(s, { type: 'point', point: p(0, 0), at: 1_000 });
    s = geofenceDrawReducer(s, { type: 'point', point: p(1, 1), at: 1_050 });
    s = geofenceDrawReducer(s, { type: 'point', point: p(2, 2), at: 1_100 });
    // Far away: a new vertex, not a close.
    expect(geofenceDrawReducer(s, { type: 'point', point: p(5, 5), at: 1_150 })?.closed).toBe(
      false,
    );
    // Same spot but after the window: also just a vertex.
    expect(geofenceDrawReducer(s, { type: 'point', point: p(2, 2), at: 9_999 })?.closed).toBe(
      false,
    );
  });

  it('closes a box on the second click with an ordered four-corner ring', () => {
    let s = geofenceDrawReducer(null, { type: 'start', mode: 'box' });
    s = geofenceDrawReducer(s, { type: 'point', point: p(45, -73), at: 1_000 });
    s = geofenceDrawReducer(s, { type: 'point', point: p(41, -70), at: 1_090 });
    expect(s?.closed).toBe(true);
    expect(geofencePoints(s as never)).toEqual([
      { lat: 41, lon: -73 },
      { lat: 41, lon: -70 },
      { lat: 45, lon: -70 },
      { lat: 45, lon: -73 },
    ]);
  });

  it('refuses degenerate boxes and keeps the draw open', () => {
    let s = geofenceDrawReducer(null, { type: 'start', mode: 'box' });
    s = geofenceDrawReducer(s, { type: 'point', point: p(10, 10), at: 1_000 });
    // Same latitude: a line, not a region.
    s = geofenceDrawReducer(s, { type: 'point', point: p(10, 20), at: 1_050 });
    expect(s?.closed).toBe(false);
    expect(rectanglePoints(p(10, 10), p(10, 20))).toBeNull();
  });

  it('undoes the last vertex and cancels', () => {
    let s = geofenceDrawReducer(null, { type: 'start', mode: 'polygon' });
    s = geofenceDrawReducer(s, { type: 'point', point: p(0, 0), at: 1_000 });
    s = geofenceDrawReducer(s, { type: 'point', point: p(1, 1), at: 1_050 });
    s = geofenceDrawReducer(s, { type: 'undo' });
    expect(s?.points).toHaveLength(1);
    expect(geofenceDrawReducer(s, { type: 'cancel' })).toBeNull();
  });

  it('previews the growing box from the first corner and the cursor', () => {
    let s = geofenceDrawReducer(null, { type: 'start', mode: 'box' });
    expect(geofencePreviewPoints(s as never)).toEqual([]);
    s = geofenceDrawReducer(s, { type: 'point', point: p(0, 0), at: 1_000 });
    s = geofenceDrawReducer(s, { type: 'cursor', point: p(2, 2) });
    expect(geofencePreviewPoints(s as never)).toEqual([
      { lat: 0, lon: 0 },
      { lat: 0, lon: 2 },
      { lat: 2, lon: 2 },
      { lat: 2, lon: 0 },
    ]);
  });
});
