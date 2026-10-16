import { describe, expect, it } from 'vitest';
import { aircraft, satellite } from '../app/testing';
import type { FeedId, LayerFamily } from '../contracts';
import { aircraftFacts, earthquakeFacts, satelliteFacts } from '../features/inspector';
import { familyLabel, familySwatch, feedGlyph, groupLayers, itemNoun } from '../features/layers';
import { commercialWords, healthSpec, licenceModeNotice } from '../features/sources';
import { layer } from '../test/fakes';
import { lookup } from './lookup';

describe('lookup', () => {
  it('returns own properties and falls back otherwise, never inherited ones', () => {
    const table: Record<string, string> = { a: 'A' };
    expect(lookup<string>(table, 'a', '?')).toBe('A');
    expect(lookup<string>(table, 'b', '?')).toBe('?');
    expect(lookup<string>(table, 'constructor', '?')).toBe('?');
    expect(lookup<string>(table, 'toString', '?')).toBe('?');
  });
});

describe('values from a newer server or a provider', () => {
  it('render with neutral fallbacks instead of throwing', () => {
    expect(familyLabel('oceans')).toBe('Oceans');
    expect(familySwatch('oceans')).toBe('context');
    expect(feedGlyph('future-feed')).toBeUndefined();
    expect(itemNoun('future-feed' as FeedId, 2)).toBe('items');
    expect(healthSpec('quantum').word).toBe('Health unknown');
    expect(commercialWords('maybe')).toBe('Unknown');
    expect(licenceModeNotice('partner').title).toBe('Commercial mode');
  });

  it('keeps layers of unknown families, after the known ones', () => {
    const groups = groupLayers(
      [
        layer('aircraft', { name: 'Ships', family: 'oceans' as LayerFamily }),
        layer('earthquakes', { name: 'Earthquakes' }),
      ],
      '',
    );
    expect(groups.map((g) => g.label)).toEqual(['Hazards', 'Oceans']);
  });

  it('does not read prototype properties from provider strings', () => {
    const quake = earthquakeFacts(
      {
        id: 'x',
        magnitude: 1,
        depthKm: 1,
        position: { lat: 0, lon: 0 },
        time: '2026-10-11T00:00:00Z',
        tsunami: false,
        alert: 'constructor',
      },
      0,
    ).find((f) => f.label === 'PAGER alert');
    expect(quake).toMatchObject({ value: 'Constructor', tone: 'neutral' });
    const group = satelliteFacts(satellite(1, { group: 'toString' })).find(
      (f) => f.label === 'Group',
    );
    expect(group?.value).toBe('toString');
    expect(aircraftFacts(aircraft('x', { emergency: 'constructor' }))[9]?.value).toBe(
      'Declared: constructor',
    );
  });
});
