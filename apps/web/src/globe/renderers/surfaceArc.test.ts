import { describe, expect, it } from 'vitest';
import { surfaceArcPositions } from './surfaceArc';

describe('bounded surface arcs', () => {
  it('keeps every long-arc chord above Earth, including terminal and mixed-length joins', () => {
    for (const coordinates of [
      [
        [0, 0],
        [90, 0],
      ],
      [
        [0, 0],
        [178, 0],
      ],
      [
        [0, 0],
        [90, 0],
        [90.1, 0],
      ],
    ] as const) {
      const positions = surfaceArcPositions(coordinates, { remaining: 1_000 });
      expect(positions).not.toBeNull();
      for (let i = 1; i < (positions?.length ?? 0); i++) {
        const from = positions?.[i - 1];
        const to = positions?.[i];
        if (!from || !to) throw new Error('Expected arc vertex');
        const midpoint = Cartesian3.midpoint(from, to, new Cartesian3());
        expect(Cartographic.fromCartesian(midpoint).height).toBeGreaterThanOrEqual(0);
      }
    }
  });
  it('refuses antipodal, coincident and malformed paths without consuming a shared budget', () => {
    const budget = { remaining: 1_000 };
    for (const end of [179.5, 179.9, 180])
      expect(
        surfaceArcPositions(
          [
            [0, 0],
            [end, 0],
          ],
          budget,
        ),
      ).toBeNull();
    expect(
      surfaceArcPositions(
        [
          [0, 0],
          [0, 0],
        ],
        budget,
      ),
    ).toBeNull();
    expect(
      surfaceArcPositions(
        [
          [0, 0],
          [Number.NaN, 0],
        ],
        budget,
      ),
    ).toBeNull();
    expect(budget.remaining).toBe(1_000);
  });
  it('preserves dateline arcs and bounds expansion across separate parts', () => {
    const budget = { remaining: 8 };
    const first = surfaceArcPositions(
      [
        [179, 0],
        [-179, 0],
      ],
      budget,
    );
    expect(first).not.toBeNull();
    expect(first?.length).toBeLessThanOrEqual(8);
    expect(budget.remaining).toBe(8 - (first?.length ?? 0));
    expect(
      surfaceArcPositions(
        [
          [0, 0],
          [90, 0],
        ],
        budget,
      ),
    ).toBeNull();
    expect(budget.remaining).toBe(8 - (first?.length ?? 0));
  });
});

import { Cartesian3, Cartographic } from '@cesium/core';
