/**
 * Public radio station renderer.
 * Displays georeferenced stations as category-colored points, with a bounded set of
 * labels for the stations nearest the camera. Clustering helpers are ported from the
 * legacy Radio layer (`legacy/src/layers/radio/clustering.js`).
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
import type { RadioCategory, RadioStation } from '../../contracts';
import { KeyedEntries } from './keyed';
import type { Located, RenderContext } from './types';

const STATION_POINT_SIZE = 7;
const STATION_SELECTED_SIZE = 13;
const LABEL_MAX_DISTANCE_M = 6_000_000;
const LABEL_OFFSET = new Cartesian2(0, -12);
const LABEL_MAX_CHARS = 30;

/** Camera height at or above which the global singleton-label allowance applies. */
export const GLOBAL_RADIO_ALTITUDE_M = 2_000_000;
export const RADIO_SINGLETON_GLOBAL_LIMIT = 16;
export const RADIO_SINGLETON_MID_LIMIT = 32;
export const RADIO_SINGLETON_NEAR_LIMIT = 48;
export const RADIO_OVERLAY_COHORT_LIMIT = 64;

const PUBLIC_SAFETY = '#f85149';
const UTILITY = '#e3b341';
const NEWS = '#58a6ff';
const TALK = '#d2a8ff';
const MUSIC = '#3fb950';

/** Marker color per contract category, grouped into the five NoirSystems radio classes. */
export const RADIO_CATEGORY_COLORS: Readonly<Record<RadioCategory, string>> = {
  emergency: PUBLIC_SAFETY,
  scanner: PUBLIC_SAFETY,
  aviation: UTILITY,
  marine: UTILITY,
  weather: UTILITY,
  traffic: UTILITY,
  news: NEWS,
  talk: TALK,
  general: MUSIC,
};

export function radioCategoryColor(category: RadioCategory): string {
  return RADIO_CATEGORY_COLORS[category] ?? MUSIC;
}

/** Return the bounded singleton-label allowance for the current camera scale. */
export function radioSingletonLabelLimit(cameraHeightM: number): number {
  const height = Math.max(0, Number(cameraHeightM) || 0);
  if (height >= GLOBAL_RADIO_ALTITUDE_M) return RADIO_SINGLETON_GLOBAL_LIMIT;
  if (height >= 250_000) return RADIO_SINGLETON_MID_LIMIT;
  return RADIO_SINGLETON_NEAR_LIMIT;
}

export interface RadioSingletonCandidate {
  readonly id?: string;
  readonly station?: { readonly id?: string };
  readonly distanceM?: number;
}

/** Rank visible singleton stations by camera distance and stable station id. */
export function selectRadioSingletonCandidates<T extends RadioSingletonCandidate>(
  candidates: readonly T[] | null | undefined,
  limit: number = RADIO_SINGLETON_NEAR_LIMIT,
): T[] {
  const distance = (c: T): number => {
    const v = Number(c?.distanceM);
    return Number.isFinite(v) ? v : Number.POSITIVE_INFINITY;
  };
  const key = (c: T): string => String(c?.station?.id || c?.id || '');
  return [...(candidates ?? [])]
    .sort((a, b) => {
      const da = distance(a);
      const db = distance(b);
      if (da !== db) return da < db ? -1 : 1;
      return key(a).localeCompare(key(b));
    })
    .slice(0, Math.max(0, Math.floor(Number(limit) || 0)));
}

export interface RadioClusterCandidate {
  readonly id?: string;
  readonly identityId?: string;
  readonly stationIds?: readonly string[];
  readonly stationCount?: number;
}

/** Rank and cap ambient clusters by station count, then id. */
export function selectRadioClusterCandidates<T extends RadioClusterCandidate>(
  candidates: readonly T[] | null | undefined,
  limit: number = RADIO_OVERLAY_COHORT_LIMIT,
): T[] {
  return [...(candidates ?? [])]
    .sort(
      (a, b) =>
        (Number(b?.stationCount) || 0) - (Number(a?.stationCount) || 0) ||
        String(a?.id || '').localeCompare(String(b?.id || '')),
    )
    .slice(0, Math.max(0, Math.floor(Number(limit) || 0)));
}

export type ReconciledRadioCluster<T extends RadioClusterCandidate> = Omit<T, 'stationIds'> & {
  readonly stationIds: string[];
  readonly membershipId: string;
  readonly identityId: string;
  readonly id: string;
};

interface CurrentCluster<T extends RadioClusterCandidate> {
  readonly source: T;
  readonly stationIds: string[];
  readonly membershipId: string;
  readonly canonicalKey: string;
}

/**
 * Preserve one-to-one overlay identity for overlapping clusters. Inheritance is
 * mutual-best: a current cluster accepts only one of its greatest contributors and a
 * prior identity transfers only to one of its strongest split children.
 */
export function reconcileRadioClusterCandidates<T extends RadioClusterCandidate>(
  candidates: readonly T[] | null | undefined,
  previous: readonly RadioClusterCandidate[] = [],
  createId: ((candidate: T, index: number) => string) | null = null,
): ReconciledRadioCluster<T>[] {
  const list = candidates ?? [];
  const current: CurrentCluster<T>[] = list.map((candidate) => {
    const stationIds = [
      ...new Set((candidate?.stationIds ?? []).map((id: string) => String(id))),
    ].sort();
    const membershipId = String(candidate?.id || `membership:${stationIds.join('|')}`);
    return {
      source: candidate,
      stationIds,
      membershipId,
      canonicalKey: `${membershipId}\u0000${stationIds.join('\u0000')}`,
    };
  });

  const membershipByIdentity = new Map<string, Set<string>>();
  for (const candidate of Array.isArray(previous) ? previous : []) {
    const identityId = String(candidate?.identityId || candidate?.id || '');
    if (!identityId) continue;
    let membership = membershipByIdentity.get(identityId);
    if (!membership) {
      membership = new Set();
      membershipByIdentity.set(identityId, membership);
    }
    for (const stationId of candidate?.stationIds ?? []) membership.add(String(stationId));
  }
  const prior = [...membershipByIdentity]
    .map(([identityId, ids]) => ({ identityId, stationIds: [...ids].sort() }))
    .filter((c) => c.stationIds.length > 0)
    .sort((a, b) => a.identityId.localeCompare(b.identityId));
  const priorByStation = new Map<string, number[]>();
  prior.forEach((p, priorIndex) => {
    for (const stationId of p.stationIds) {
      const list = priorByStation.get(stationId);
      if (list) list.push(priorIndex);
      else priorByStation.set(stationId, [priorIndex]);
    }
  });

  interface Edge {
    currentIndex: number;
    priorIndex: number;
    overlap: number;
    score: number;
    similarity: number;
  }
  const edges: Edge[] = [];
  const greatestByCurrent = new Map<number, number>();
  const greatestByPrior = new Map<number, number>();
  current.forEach((cur, currentIndex) => {
    const overlapByPrior = new Map<number, number>();
    for (const stationId of cur.stationIds) {
      for (const priorIndex of priorByStation.get(stationId) ?? []) {
        overlapByPrior.set(priorIndex, (overlapByPrior.get(priorIndex) ?? 0) + 1);
      }
    }
    for (const [priorIndex, overlap] of overlapByPrior) {
      const priorSize = prior[priorIndex]?.stationIds.length ?? 0;
      const union = cur.stationIds.length + priorSize - overlap;
      const smaller = Math.min(cur.stationIds.length, priorSize);
      edges.push({
        currentIndex,
        priorIndex,
        overlap,
        score: smaller > 0 ? overlap / smaller : 0,
        similarity: union > 0 ? overlap / union : 0,
      });
      greatestByCurrent.set(
        currentIndex,
        Math.max(greatestByCurrent.get(currentIndex) ?? 0, overlap),
      );
      greatestByPrior.set(priorIndex, Math.max(greatestByPrior.get(priorIndex) ?? 0, overlap));
    }
  });

  const identityAt = (i: number): string => prior[i]?.identityId ?? '';
  const keyAt = (i: number): string => current[i]?.canonicalKey ?? '';
  const eligible = edges
    .filter(
      (e) =>
        e.overlap === greatestByCurrent.get(e.currentIndex) &&
        e.overlap === greatestByPrior.get(e.priorIndex),
    )
    // Identity follows the contributor representing the largest share of the new cluster.
    .sort(
      (a, b) =>
        b.overlap - a.overlap ||
        b.score - a.score ||
        b.similarity - a.similarity ||
        keyAt(a.currentIndex).localeCompare(keyAt(b.currentIndex)) ||
        identityAt(a.priorIndex).localeCompare(identityAt(b.priorIndex)),
    );

  const inheritedByCurrent = new Map<number, string>();
  const usedPrior = new Set<string>();
  for (const edge of eligible) {
    const identityId = identityAt(edge.priorIndex);
    if (inheritedByCurrent.has(edge.currentIndex) || usedPrior.has(identityId)) continue;
    inheritedByCurrent.set(edge.currentIndex, identityId);
    usedPrior.add(identityId);
  }

  const generatedByCurrent = new Map<number, string>();
  if (typeof createId === 'function') {
    const fresh = current
      .map((cur, index) => ({ cur, index }))
      .filter(({ index }) => !inheritedByCurrent.has(index))
      .sort((a, b) => a.cur.canonicalKey.localeCompare(b.cur.canonicalKey));
    for (const { cur, index } of fresh) {
      generatedByCurrent.set(index, String(createId(cur.source, index) || ''));
    }
  }

  return current.map((cur, index) => {
    const identityId =
      inheritedByCurrent.get(index) || generatedByCurrent.get(index) || cur.membershipId;
    return {
      ...cur.source,
      stationIds: cur.stationIds,
      membershipId: cur.membershipId,
      identityId,
      id: identityId,
    };
  });
}

function truncateLabel(name: string): string {
  const chars = [...name];
  return chars.length > LABEL_MAX_CHARS ? `${chars.slice(0, LABEL_MAX_CHARS - 1).join('')}…` : name;
}

interface RadioEntry {
  readonly point: PointPrimitive;
  readonly label: Label;
}

export class RadioRenderer {
  readonly #ctx: RenderContext;
  readonly #primitives: PrimitiveCollection;
  readonly #points: PointPrimitiveCollection;
  readonly #labels: LabelCollection;
  readonly #stations = new Map<string, RadioStation>();
  #selectedId: string | null = null;
  readonly #entries: KeyedEntries<RadioStation, RadioEntry>;

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

    this.#entries = new KeyedEntries<RadioStation, RadioEntry>((s) => s.id, {
      create: (station, id) => {
        this.#stations.set(id, station);
        const pos = Cartesian3.fromDegrees(station.position.lon, station.position.lat, 0);
        const ref = { feed: 'radio', id };
        const point = this.#points.add({
          position: pos,
          color: this.#colorFor(station, id),
          pixelSize: id === this.#selectedId ? STATION_SELECTED_SIZE : STATION_POINT_SIZE,
          id: ref,
        });
        const label = this.#labels.add({
          position: pos,
          text: truncateLabel(station.name),
          font: '10px Inter, sans-serif',
          fillColor: Color.WHITE,
          outlineColor: Color.BLACK,
          outlineWidth: 2,
          style: LabelStyle.FILL_AND_OUTLINE,
          pixelOffset: LABEL_OFFSET,
          distanceDisplayCondition: new DistanceDisplayCondition(0, LABEL_MAX_DISTANCE_M),
          show: false,
          id: ref,
        });
        return { point, label };
      },
      update: (entry, station, id) => {
        this.#stations.set(id, station);
        const pos = Cartesian3.fromDegrees(station.position.lon, station.position.lat, 0);
        entry.point.position = pos;
        entry.point.color = this.#colorFor(station, id);
        entry.point.pixelSize =
          id === this.#selectedId ? STATION_SELECTED_SIZE : STATION_POINT_SIZE;
        entry.label.position = pos;
        entry.label.text = truncateLabel(station.name);
      },
      remove: (entry, id) => {
        this.#stations.delete(id);
        this.#points.remove(entry.point);
        this.#labels.remove(entry.label);
      },
    });
  }

  #colorFor(station: RadioStation, id: string): Color {
    if (id === this.#selectedId) return this.#ctx.colors.selection;
    return Color.fromCssColorString(radioCategoryColor(station.category)) ?? Color.WHITE;
  }

  setStations(stations: readonly RadioStation[]): void {
    this.#entries.apply(stations);
    this.refreshLabels();
    this.#ctx.requestRender();
  }

  /** Show labels only for the stations nearest the camera, bounded by camera height. */
  refreshLabels(): void {
    const camera = this.#ctx.cameraPosition();
    const candidates = [...this.#entries.entries()].map(([id, entry]) => ({
      id,
      entry,
      distanceM: Cartesian3.distance(camera, entry.point.position),
    }));
    const shown = new Set(
      selectRadioSingletonCandidates(
        candidates,
        radioSingletonLabelLimit(this.#ctx.cameraHeightM()),
      ).map((c) => c.id),
    );
    for (const [id, entry] of this.#entries.entries()) {
      entry.label.show = shown.has(id) || id === this.#selectedId;
    }
  }

  getStation(id: string): RadioStation | undefined {
    return this.#stations.get(id);
  }

  setSelected(id: string | null): void {
    if (this.#selectedId === id) return;
    this.#selectedId = id;
    for (const [stationId, entry] of this.#entries.entries()) {
      const station = this.#stations.get(stationId);
      const isSelected = stationId === id;
      if (station) entry.point.color = this.#colorFor(station, stationId);
      entry.point.pixelSize = isSelected ? STATION_SELECTED_SIZE : STATION_POINT_SIZE;
    }
    this.refreshLabels();
    this.#ctx.requestRender();
  }

  locate(id: string): Located | undefined {
    const station = this.#stations.get(id);
    if (!station) return undefined;
    return {
      position: Cartesian3.fromDegrees(station.position.lon, station.position.lat, 0),
      symbolPx: STATION_SELECTED_SIZE,
    };
  }

  clear(): void {
    this.#entries.clear();
    this.#stations.clear();
    this.#ctx.requestRender();
  }

  destroy(): void {
    this.#entries.clear();
    this.#stations.clear();
    this.#primitives.remove(this.#points);
    this.#primitives.remove(this.#labels);
  }
}
