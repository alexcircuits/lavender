import { beforeAll, describe, expect, it } from 'vitest';
import { arbitrateLabelsWasm, type LabelCandidateInput, loadLavenderWasm } from './wasmLabels';

describe('wasmLabels (Screen-Space Label Collision Arbiter)', () => {
  beforeAll(async () => {
    await loadLavenderWasm();
  });

  it('accepts all non-overlapping labels', () => {
    const candidates: LabelCandidateInput[] = [
      { x: 10, y: 10, w: 50, h: 20, priority: 10 },
      { x: 100, y: 10, w: 50, h: 20, priority: 20 },
      { x: 10, y: 100, w: 50, h: 20, priority: 30 },
      { x: 100, y: 100, w: 50, h: 20, priority: 40 },
    ];

    const result = arbitrateLabelsWasm(candidates, 1000, 800);
    expect(result.visibleCount).toBe(4);
    expect(result.visible).toEqual([true, true, true, true]);
  });

  it('suppresses lower-priority overlapping label', () => {
    const candidates: LabelCandidateInput[] = [
      { x: 50, y: 50, w: 60, h: 20, priority: 10 }, // overlaps with [1]
      { x: 60, y: 55, w: 60, h: 20, priority: 50 }, // higher priority wins
    ];

    const result = arbitrateLabelsWasm(candidates, 1000, 800);
    expect(result.visibleCount).toBe(1);
    expect(result.visible).toEqual([false, true]);
  });

  it('selected label always wins over overlapping label even with lower numeric priority', () => {
    const candidates: LabelCandidateInput[] = [
      { x: 100, y: 100, w: 80, h: 30, priority: 1, isSelected: true },
      { x: 120, y: 110, w: 80, h: 30, priority: 999999, isSelected: false },
    ];

    const result = arbitrateLabelsWasm(candidates, 1000, 800);
    expect(result.visibleCount).toBe(1);
    expect(result.visible[0]).toBe(true);
    expect(result.visible[1]).toBe(false);
  });

  it('tracked label wins over normal label when both overlap', () => {
    const candidates: LabelCandidateInput[] = [
      { x: 200, y: 200, w: 50, h: 20, priority: 10, isTracked: true },
      { x: 210, y: 205, w: 50, h: 20, priority: 100, isTracked: false },
    ];

    const result = arbitrateLabelsWasm(candidates, 1000, 800);
    expect(result.visibleCount).toBe(1);
    expect(result.visible).toEqual([true, false]);
  });

  it('selected beats tracked when both collide', () => {
    const candidates: LabelCandidateInput[] = [
      { x: 300, y: 300, w: 50, h: 20, priority: 1, isSelected: true, isTracked: false },
      { x: 310, y: 305, w: 50, h: 20, priority: 999, isSelected: false, isTracked: true },
    ];

    const result = arbitrateLabelsWasm(candidates, 1000, 800);
    expect(result.visibleCount).toBe(1);
    expect(result.visible).toEqual([true, false]);
  });

  it('enforces maxLabels budget cap', () => {
    const candidates: LabelCandidateInput[] = [
      { x: 10, y: 10, w: 20, h: 10, priority: 10 },
      { x: 40, y: 10, w: 20, h: 10, priority: 50 },
      { x: 70, y: 10, w: 20, h: 10, priority: 30 },
      { x: 100, y: 10, w: 20, h: 10, priority: 40 },
    ];

    // Cap at 2
    const result = arbitrateLabelsWasm(candidates, 1000, 800, 2);
    expect(result.visibleCount).toBe(2);
    // Highest priorities: 50 (index 1) and 40 (index 3)
    expect(result.visible[1]).toBe(true);
    expect(result.visible[3]).toBe(true);
    expect(result.visible[0]).toBe(false);
    expect(result.visible[2]).toBe(false);
  });

  it('handles off-screen candidates cleanly', () => {
    const candidates: LabelCandidateInput[] = [
      { x: -100, y: 50, w: 40, h: 20 }, // completely off-screen left
      { x: 1200, y: 50, w: 40, h: 20 }, // completely off-screen right
      { x: 50, y: -50, w: 40, h: 20 }, // completely off-screen top
      { x: 50, y: 900, w: 40, h: 20 }, // completely off-screen bottom
      { x: 50, y: 50, w: 40, h: 20 }, // on-screen
    ];

    const result = arbitrateLabelsWasm(candidates, 1000, 800);
    expect(result.visibleCount).toBe(1);
    expect(result.visible).toEqual([false, false, false, false, true]);
  });

  it('handles empty candidates or invalid screen dimensions gracefully', () => {
    expect(arbitrateLabelsWasm([], 1000, 800)).toEqual({ visible: [], visibleCount: 0 });
    expect(arbitrateLabelsWasm([{ x: 10, y: 10, w: 20, h: 10 }], 0, 800)).toEqual({
      visible: [false],
      visibleCount: 0,
    });
    expect(arbitrateLabelsWasm([{ x: 10, y: 10, w: 20, h: 10 }], 1000, -100)).toEqual({
      visible: [false],
      visibleCount: 0,
    });
  });
});
