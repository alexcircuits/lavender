import { beforeAll, describe, expect, it } from 'vitest';
import { loadLavenderWasm } from '../../globe/renderers/wasmGeometry';
import type { MapAnnotation } from '../annotations/model';
import { type EntityRecord, evaluateGeofences, makeTrackingKey } from './geofenceMonitor';

describe('geofenceMonitor', () => {
  beforeAll(async () => {
    await loadLavenderWasm();
  });

  const sampleFence: MapAnnotation = {
    id: 'fence-1',
    kind: 'geofence',
    label: 'Restricted Airspace Alpha',
    note: 'Active military airspace',
    createdAt: '2026-10-25T12:00:00.000Z',
    approximate: true,
    points: [
      { lat: 10, lon: 10 },
      { lat: 10, lon: 20 },
      { lat: 20, lon: 20 },
      { lat: 20, lon: 10 },
    ],
  };

  it('emits enter alert when entity transitions from outside to inside', () => {
    const insideEntity: EntityRecord = {
      id: 'plane-1',
      feed: 'aircraft',
      position: { lat: 15, lon: 15 },
      label: 'UA101',
    };

    // Outside previously (tracking map marked false)
    const prevTracking = new Map([[makeTrackingKey(sampleFence.id, insideEntity.id), false]]);

    const result = evaluateGeofences([insideEntity], [sampleFence], prevTracking);

    expect(result.alerts).toHaveLength(1);
    expect(result.alerts[0]?.type).toBe('enter');
    expect(result.alerts[0]?.entity.id).toBe('plane-1');
    expect(result.alerts[0]?.geofenceId).toBe('fence-1');
    expect(result.alerts[0]?.geofenceName).toBe('Restricted Airspace Alpha');
    expect(result.alerts[0]?.position).toEqual({ lat: 15, lon: 15 });
    expect(result.nextTracking.get(makeTrackingKey(sampleFence.id, insideEntity.id))).toBe(true);
  });

  it('emits enter alert on first observation when entity starts inside with empty tracking', () => {
    const insideEntity: EntityRecord = {
      id: 'plane-1',
      feed: 'aircraft',
      position: { lat: 15, lon: 15 },
      label: 'UA101',
    };

    const result = evaluateGeofences([insideEntity], [sampleFence]);

    expect(result.alerts).toHaveLength(1);
    expect(result.alerts[0]?.type).toBe('enter');
    expect(result.nextTracking.get(makeTrackingKey(sampleFence.id, insideEntity.id))).toBe(true);
  });

  it('emits nothing when entity stays inside', () => {
    const insideEntity: EntityRecord = {
      id: 'plane-1',
      feed: 'aircraft',
      position: { lat: 16, lon: 16 },
      label: 'UA101',
    };

    const prevTracking = new Map([[makeTrackingKey(sampleFence.id, insideEntity.id), true]]);

    const result = evaluateGeofences([insideEntity], [sampleFence], prevTracking);

    expect(result.alerts).toHaveLength(0);
    expect(result.nextTracking.get(makeTrackingKey(sampleFence.id, insideEntity.id))).toBe(true);
  });

  it('emits exit alert when entity transitions from inside to outside', () => {
    const outsideEntity: EntityRecord = {
      id: 'plane-1',
      feed: 'aircraft',
      position: { lat: 5, lon: 5 },
      label: 'UA101',
    };

    const prevTracking = new Map([[makeTrackingKey(sampleFence.id, outsideEntity.id), true]]);

    const result = evaluateGeofences([outsideEntity], [sampleFence], prevTracking);

    expect(result.alerts).toHaveLength(1);
    expect(result.alerts[0]?.type).toBe('exit');
    expect(result.alerts[0]?.entity.id).toBe('plane-1');
    expect(result.alerts[0]?.geofenceId).toBe('fence-1');
    expect(result.alerts[0]?.geofenceName).toBe('Restricted Airspace Alpha');
    expect(result.alerts[0]?.position).toEqual({ lat: 5, lon: 5 });
    expect(result.nextTracking.get(makeTrackingKey(sampleFence.id, outsideEntity.id))).toBe(false);
  });

  it('emits nothing when entity stays outside', () => {
    const outsideEntity: EntityRecord = {
      id: 'plane-1',
      feed: 'aircraft',
      position: { lat: 5, lon: 5 },
      label: 'UA101',
    };

    const prevTracking = new Map([[makeTrackingKey(sampleFence.id, outsideEntity.id), false]]);

    const result = evaluateGeofences([outsideEntity], [sampleFence], prevTracking);

    expect(result.alerts).toHaveLength(0);
    expect(result.nextTracking.get(makeTrackingKey(sampleFence.id, outsideEntity.id))).toBe(false);
  });

  it('supports direct lat and lon properties on entity records', () => {
    const vessel: EntityRecord = {
      id: 'vessel-99',
      feed: 'vessels',
      lat: 12,
      lon: 14,
      name: 'Cargo Voyager',
    };

    const result = evaluateGeofences([vessel], [sampleFence]);

    expect(result.alerts).toHaveLength(1);
    expect(result.alerts[0]?.type).toBe('enter');
    expect(result.alerts[0]?.entity.label).toBe('Cargo Voyager');
    expect(result.alerts[0]?.position).toEqual({ lat: 12, lon: 14 });
  });

  it('filters out non-geofence annotations such as pins and lines', () => {
    const pin: MapAnnotation = {
      id: 'pin-1',
      kind: 'pin',
      label: 'Observation Point',
      note: '',
      createdAt: '2026-10-25T12:00:00.000Z',
      approximate: true,
      points: [{ lat: 15, lon: 15 }],
    };

    const plane: EntityRecord = {
      id: 'plane-1',
      feed: 'aircraft',
      position: { lat: 15, lon: 15 },
    };

    const result = evaluateGeofences([plane], [pin]);
    expect(result.alerts).toHaveLength(0);
  });
});
