interface Rectangle {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}
export interface LabelAnchor {
  readonly x: number;
  readonly y: number;
  readonly width: number;
}
const overlap = (a: Rectangle, b: Rectangle) =>
  a.x < b.x + b.width && a.x + a.width > b.x && a.y < b.y + b.height && a.y + a.height > b.y;
/** Eight readable labels at most; hidden labels remain fully represented in the list. */
export function placeAnnotationLabels(
  anchors: readonly LabelAnchor[],
  width: number,
  height: number,
): readonly (number | null)[] {
  const occupied: Rectangle[] = [];
  return anchors.map((anchor) => {
    if (occupied.length >= 8 || !Number.isFinite(anchor.x) || !Number.isFinite(anchor.y))
      return null;
    for (const offset of [-24, -64, 16, -104, 56, -144, 96, -184, 136]) {
      const box: Rectangle = {
        x: anchor.x - anchor.width / 2 - 6,
        y: anchor.y + offset - 16,
        width: anchor.width + 12,
        height: 34,
      };
      if (
        box.x < 8 ||
        box.y < 8 ||
        box.x + box.width > width - 8 ||
        box.y + box.height > height - 8 ||
        occupied.some((other) => overlap(box, other))
      )
        continue;
      occupied.push(box);
      return offset;
    }
    return null;
  });
}
