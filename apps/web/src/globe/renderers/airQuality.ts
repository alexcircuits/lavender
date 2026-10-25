/**
 * Air Quality (AQHI) observation station renderer.
 * Displays realtime environmental health monitoring stations, color-coded by AQHI health risk band.
 */
import { Cartesian2, Cartesian3, Color, DistanceDisplayCondition } from '@cesium/core';
import {
  type Label,
  LabelCollection,
  LabelStyle,
  type PointPrimitive,
  PointPrimitiveCollection,
  type PrimitiveCollection,
} from '@cesium/engine';
import type { AirQualityReading } from '../../contracts';
import type { EntityRef } from '../index';
import { KeyedEntries } from './keyed';
import type { Located, RenderContext } from './types';

const STATION_POINT_SIZE = 8;
const STATION_SELECTED_SIZE = 14;
const LABEL_MAX_DISTANCE_M = 1_500_000;
const LABEL_OFFSET = new Cartesian2(0, -12);

interface AirQualityEntry {
  readonly point: PointPrimitive;
  readonly label: Label;
}

export class AirQualityRenderer {
  readonly #ctx: RenderContext;
  readonly #primitives: PrimitiveCollection;
  readonly #points: PointPrimitiveCollection;
  readonly #labels: LabelCollection;
  readonly #readings = new Map<string, AirQualityReading>();
  #selectedId: string | null = null;
  readonly #entries: KeyedEntries<AirQualityReading, AirQualityEntry>;

  constructor(
    primitives: PrimitiveCollection,
    ctx: RenderContext,
    points?: PointPrimitiveCollection,
    labels?: LabelCollection,
  ) {
    this.#ctx = ctx;
    this.#primitives = primitives;
    this.#points = points ?? primitives.add(new PointPrimitiveCollection());
    this.#labels = labels ?? primitives.add(new LabelCollection());

    this.#entries = new KeyedEntries<AirQualityReading, AirQualityEntry>((r) => r.id, {
      create: (reading, id) => {
        this.#readings.set(id, reading);
        const isSelected = id === this.#selectedId;
        const color = isSelected
          ? this.#ctx.colors.selection
          : (Color.fromCssColorString(reading.color) ?? Color.CYAN);
        const pos = Cartesian3.fromDegrees(reading.longitude, reading.latitude, 0);

        const point = this.#points.add({
          position: pos,
          color,
          pixelSize: isSelected ? STATION_SELECTED_SIZE : STATION_POINT_SIZE,
          id: { feed: 'air-quality', id } satisfies EntityRef,
        });

        const label = this.#labels.add({
          position: pos,
          text: reading.label,
          font: '10px Inter, sans-serif',
          fillColor: Color.WHITE,
          outlineColor: Color.BLACK,
          outlineWidth: 2,
          style: LabelStyle.FILL_AND_OUTLINE,
          pixelOffset: LABEL_OFFSET,
          distanceDisplayCondition: new DistanceDisplayCondition(0, LABEL_MAX_DISTANCE_M),
          id: { feed: 'air-quality', id } satisfies EntityRef,
        });

        return { point, label };
      },
      update: (entry, reading, id) => {
        this.#readings.set(id, reading);
        const isSelected = id === this.#selectedId;
        const color = isSelected
          ? this.#ctx.colors.selection
          : (Color.fromCssColorString(reading.color) ?? Color.CYAN);
        const pos = Cartesian3.fromDegrees(reading.longitude, reading.latitude, 0);

        entry.point.position = pos;
        entry.point.color = color;
        entry.point.pixelSize = isSelected ? STATION_SELECTED_SIZE : STATION_POINT_SIZE;

        entry.label.position = pos;
        entry.label.text = reading.label;
      },
      remove: (entry, id) => {
        this.#readings.delete(id);
        this.#points.remove(entry.point);
        this.#labels.remove(entry.label);
      },
    });
  }

  setReadings(readings: readonly AirQualityReading[]): void {
    this.#entries.apply(readings);
    this.#ctx.requestRender();
  }

  getReading(id: string): AirQualityReading | undefined {
    return this.#readings.get(id);
  }

  setSelected(id: string | null): void {
    if (this.#selectedId === id) return;
    this.#selectedId = id;
    for (const [readingId, entry] of this.#entries.entries()) {
      const reading = this.#readings.get(readingId);
      const isSelected = readingId === id;
      const color = isSelected
        ? this.#ctx.colors.selection
        : (Color.fromCssColorString(reading?.color ?? '#00ccff') ?? Color.CYAN);
      entry.point.color = color;
      entry.point.pixelSize = isSelected ? STATION_SELECTED_SIZE : STATION_POINT_SIZE;
    }
    this.#ctx.requestRender();
  }

  locate(id: string): Located | undefined {
    const reading = this.#readings.get(id);
    if (!reading) return undefined;
    return {
      position: Cartesian3.fromDegrees(reading.longitude, reading.latitude, 0),
      symbolPx: STATION_SELECTED_SIZE,
    };
  }

  clear(): void {
    this.#entries.clear();
    this.#readings.clear();
    this.#ctx.requestRender();
  }

  destroy(): void {
    this.#entries.clear();
    this.#readings.clear();
    this.#primitives.remove(this.#points);
    this.#primitives.remove(this.#labels);
  }
}
