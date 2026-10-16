import {
  Cartesian2,
  Cartesian3,
  Cartographic,
  ColorGeometryInstanceAttribute,
  EllipsoidGeodesic,
  GeometryInstance,
  PolygonGeometry,
  PolygonHierarchy,
} from '@cesium/core';
import {
  GroundPrimitive,
  type Label,
  LabelCollection,
  LabelStyle,
  Material,
  PerInstanceColorAppearance,
  PointPrimitiveCollection,
  PolylineCollection,
  type PrimitiveCollection,
  type Scene,
} from '@cesium/engine';
import type { GeoPoint } from '../../contracts';
import {
  annotationCentre,
  EARTH_RADIUS_M,
  greatCircleM,
  MAX_EDGE_ANGLE_RAD,
  measurement,
} from '../../features/annotations/geometry';
import type { AnnotationDraft, MapAnnotation } from '../../features/annotations/model';
import { placeAnnotationLabels } from './annotationLabels';
import type { RenderContext } from './types';

/** Bounded user marks, redrawn only when edited; all geometry remains anchored to Earth. */
export class AnnotationRenderer {
  readonly #root: PrimitiveCollection;
  readonly #scene: Scene;
  readonly #labelEntries: Array<{ readonly label: Label; readonly width: number }> = [];
  readonly #ctx: RenderContext;
  readonly #points: PointPrimitiveCollection;
  readonly #lines: PolylineCollection;
  readonly #labels: LabelCollection;
  readonly #fills: GroundPrimitive[] = [];
  constructor(root: PrimitiveCollection, ctx: RenderContext, scene: Scene) {
    this.#root = root;
    this.#scene = scene;
    this.#ctx = ctx;
    this.#points = root.add(new PointPrimitiveCollection());
    this.#lines = root.add(new PolylineCollection());
    this.#labels = root.add(new LabelCollection());
  }
  show(annotations: readonly MapAnnotation[], draft: AnnotationDraft | null): void {
    this.#points.removeAll();
    this.#lines.removeAll();
    this.#labels.removeAll();
    this.#labelEntries.length = 0;
    for (const fill of this.#fills.splice(0)) this.#root.remove(fill);
    for (const annotation of annotations)
      this.#drawSafely(annotation.kind, annotation.points, annotation.label, false);
    if (draft) this.#drawSafely(draft.kind, draft.points, 'Draft', true);
    this.viewChanged();
    this.#ctx.requestRender();
  }
  viewChanged(): void {
    const entries = this.#labelEntries;
    const anchors = entries.map(({ label, width }) => {
      label.pixelOffset = new Cartesian2();
      let point: Cartesian2 | undefined;
      try {
        point = label.computeScreenSpacePosition(this.#scene);
      } catch (error) {
        console.error('Globe could not place one annotation label', error);
      }
      return { x: point?.x ?? Number.NaN, y: point?.y ?? Number.NaN, width };
    });
    const offsets = placeAnnotationLabels(
      anchors,
      this.#scene.canvas.clientWidth,
      this.#scene.canvas.clientHeight,
    );
    for (const [index, entry] of entries.entries()) {
      const offset = offsets[index];
      entry.label.show = offset !== null && offset !== undefined;
      if (entry.label.show) entry.label.pixelOffset = new Cartesian2(0, offset ?? 0);
    }
    this.#ctx.requestRender();
  }
  #drawSafely(
    kind: MapAnnotation['kind'],
    points: readonly GeoPoint[],
    label: string,
    draft: boolean,
  ): void {
    try {
      this.#draw(kind, points, label, draft);
    } catch (error) {
      console.error('Globe could not draw one annotation', error);
    }
  }
  #draw(
    kind: MapAnnotation['kind'],
    points: readonly GeoPoint[],
    label: string,
    draft: boolean,
  ): void {
    if (!points.length) return;
    const colour = this.#ctx.colors.selection;
    const position = (p: GeoPoint) => Cartesian3.fromDegrees(p.lon, p.lat, 25);
    if (kind === 'pin' || draft) {
      for (const point of points)
        this.#points.add({
          position: position(point),
          pixelSize: 8,
          color: colour,
          outlineColor: this.#ctx.colors.outline,
          outlineWidth: 2,
        });
    }
    if (kind !== 'pin' && points.length >= 2) {
      const ring =
        kind === 'area' && points.length >= 3 ? [...points, points[0] as GeoPoint] : points;
      const positions: Cartesian3[] = [];
      for (let i = 1; i < ring.length; i++) {
        const a = ring[i - 1] as GeoPoint;
        const b = ring[i] as GeoPoint;
        const distance = greatCircleM(a, b);
        if (distance < 0.5 || distance >= MAX_EDGE_ANGLE_RAD * EARTH_RADIUS_M) continue;
        // Densify ellipsoid arcs; coarse Cartesian chords would disappear inside Earth.
        const geodesic = new EllipsoidGeodesic(
          Cartographic.fromDegrees(a.lon, a.lat),
          Cartographic.fromDegrees(b.lon, b.lat),
        );
        const count = Math.min(512, Math.max(1, Math.ceil(geodesic.surfaceDistance / 25_000)));
        for (let j = 0; j < count; j++) {
          const p = geodesic.interpolateUsingFraction(j / count);
          p.height = 25 + (geodesic.surfaceDistance / count) ** 2 / (8 * EARTH_RADIUS_M);
          positions.push(Cartographic.toCartesian(p));
        }
      }
      positions.push(position(ring[ring.length - 1] as GeoPoint));
      if (positions.length >= 2)
        this.#lines.add({
          positions,
          width: draft ? 1.5 : kind === 'line' ? 5 : 2,
          material: Material.fromType(kind === 'line' && !draft ? 'PolylineArrow' : 'Color', {
            color: colour,
          }),
        });
    }
    if (kind === 'area' && points.length >= 3 && !draft) {
      const fill = new GroundPrimitive({
        geometryInstances: new GeometryInstance({
          geometry: new PolygonGeometry({
            polygonHierarchy: new PolygonHierarchy(points.map(position)),
            granularity: Math.PI / 180,
          }),
          attributes: { color: ColorGeometryInstanceAttribute.fromColor(colour.withAlpha(0.16)) },
        }),
        appearance: new PerInstanceColorAppearance({ translucent: true, closed: false }),
        asynchronous: true,
      });
      this.#fills.push(this.#root.add(fill));
    }
    const centre = annotationCentre(points);
    if (centre) {
      const name = label || kind;
      const title = name.length > 28 ? `${name.slice(0, 27)}…` : name;
      const measure = measurement(kind, points);
      const entry = this.#labels.add({
        position: position(centre),
        text: `${title}\n${measure}`,
        font: this.#ctx.colors.css.labelFont,
        fillColor: this.#ctx.colors.ink,
        outlineColor: this.#ctx.colors.outline,
        outlineWidth: 3,
        style: LabelStyle.FILL_AND_OUTLINE,
        pixelOffset: new Cartesian2(0, -16),
        showBackground: true,
        backgroundColor: this.#ctx.colors.outline.withAlpha(0.9),
        backgroundPadding: new Cartesian2(6, 3),
      });
      this.#labelEntries.push({ label: entry, width: Math.max(title.length, measure.length) * 7 });
    }
  }
  destroy(): void {
    for (const fill of this.#fills.splice(0)) this.#root.remove(fill);
    this.#root.remove(this.#points);
    this.#root.remove(this.#lines);
    this.#root.remove(this.#labels);
  }
}
