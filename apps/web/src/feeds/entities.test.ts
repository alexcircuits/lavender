import { describe, expect, it } from 'vitest';
import { BoatIcon } from '../design';
import { feedGlyph, itemNoun } from '../features/layers';
import { vessel, vesselSnapshot } from '../test/vessel';
import { findItem, itemId, usesViewport } from './entities';

describe('vessel snapshot identity', () => {
  it('uses MMSI so two contacts with the same name remain distinct', () => {
    const first = vessel({ name: 'TEST SHIP' });
    const second = vessel({ mmsi: '987654321', name: 'TEST SHIP' });
    const snapshot = vesselSnapshot([first, second]);
    expect(itemId(snapshot.items, first)).toBe('123456789');
    expect(findItem(snapshot.items, '987654321')).toBe(second);
    expect(findItem(snapshot.items, 'TEST SHIP')).toBeUndefined();
    expect(usesViewport('vessels')).toBe(true);
  });

  it('uses the maritime icon and vessel nouns', () => {
    expect(feedGlyph('vessels')).toBe(BoatIcon);
    expect(itemNoun('vessels', 1)).toBe('vessel');
    expect(itemNoun('vessels', 2)).toBe('vessels');
  });
});

describe('launch queries', () => {
  it('are worldwide, so the launch list does not change as the camera moves', () => {
    expect(usesViewport('launches')).toBe(false);
  });
});
