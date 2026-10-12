import { describe, expect, it } from 'vitest';
import { KeyedEntries } from './keyed';

interface Item {
  readonly id: string;
  readonly value: number;
}

function harness() {
  const log: string[] = [];
  const keyed = new KeyedEntries<Item, { id: string; value: number }>((item) => item.id, {
    create: (item, id) => {
      log.push(`+${id}`);
      return { id, value: item.value };
    },
    update: (entry, item, id) => {
      log.push(`~${id}`);
      entry.value = item.value;
    },
    remove: (_entry, id) => {
      log.push(`-${id}`);
    },
  });
  return { keyed, log };
}

describe('KeyedEntries', () => {
  it('adds new ids, updates kept ones in place and removes missing ones', () => {
    const { keyed, log } = harness();
    expect(
      keyed.apply([
        { id: 'a', value: 1 },
        { id: 'b', value: 1 },
      ]),
    ).toEqual({
      added: 2,
      updated: 0,
      removed: 0,
    });
    const b = keyed.get('b');
    log.length = 0;
    expect(
      keyed.apply([
        { id: 'b', value: 2 },
        { id: 'c', value: 1 },
      ]),
    ).toEqual({
      added: 1,
      updated: 1,
      removed: 1,
    });
    expect(log).toEqual(['~b', '+c', '-a']);
    // The same entry object survives: primitives are updated, never rebuilt.
    expect(keyed.get('b')).toBe(b);
    expect(keyed.get('b')?.value).toBe(2);
    expect(keyed.get('a')).toBeUndefined();
    expect(keyed.size).toBe(2);
  });

  it('treats a repeated id in one snapshot as one entity (last value wins)', () => {
    const { keyed } = harness();
    const result = keyed.apply([
      { id: 'a', value: 1 },
      { id: 'a', value: 2 },
    ]);
    expect(result).toEqual({ added: 1, updated: 0, removed: 0 });
    expect(keyed.size).toBe(1);
    expect(keyed.get('a')?.value).toBe(2);
  });

  it('removes everything for an empty snapshot and on clear', () => {
    const { keyed, log } = harness();
    keyed.apply([{ id: 'a', value: 1 }]);
    expect(keyed.apply([])).toEqual({ added: 0, updated: 0, removed: 1 });
    keyed.apply([
      { id: 'x', value: 1 },
      { id: 'y', value: 1 },
    ]);
    log.length = 0;
    expect(keyed.clear()).toBe(2);
    expect(log.sort()).toEqual(['-x', '-y']);
    expect(keyed.size).toBe(0);
  });

  it('diffs 10 000 entities within the frame budget', () => {
    const { keyed } = harness();
    const items = Array.from({ length: 10_000 }, (_, i) => ({ id: `id${i}`, value: i }));
    keyed.apply(items);
    const shifted = items.map((item) => ({ ...item, value: item.value + 1 }));
    const start = performance.now();
    keyed.apply(shifted);
    // The real budget is 8 ms in the browser; this generous bound only catches O(n²) mistakes.
    expect(performance.now() - start).toBeLessThan(100);
  });
});
