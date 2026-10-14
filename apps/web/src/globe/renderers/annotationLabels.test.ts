import { expect, it } from 'vitest';
import { placeAnnotationLabels } from './annotationLabels';

it('separates coincident labels and keeps a bounded viewport budget', () => {
  const positions = placeAnnotationLabels(
    Array.from({ length: 24 }, () => ({ x: 500, y: 400, width: 180 })),
    1_000,
    800,
  );
  const visible = positions.filter((p) => p !== null);
  expect(visible).toHaveLength(8);
  expect(new Set(visible).size).toBe(8);
  for (let i = 1; i < visible.length; i++)
    for (let j = 0; j < i; j++)
      expect(Math.abs(Number(visible[i]) - Number(visible[j]))).toBeGreaterThanOrEqual(34);
});
it('suppresses off-screen and invalid anchors instead of placing unreadable labels', () => {
  expect(
    placeAnnotationLabels(
      [
        { x: 10, y: 10, width: 200 },
        { x: Number.NaN, y: 400, width: 180 },
      ],
      1_000,
      800,
    ),
  ).toEqual([null, null]);
});
