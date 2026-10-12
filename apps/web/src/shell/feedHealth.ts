import type { Catalog, FeedId } from '../contracts';
import { isActive, type LayerConditionKind, layerCondition, SEVERITY } from '../features/layers';
import type { FeedSummary, LayerState } from '../state/stores';

export interface FeedHealth {
  /** Layers that are on and backed by an available source. */
  readonly active: number;
  readonly counts: Readonly<Partial<Record<LayerConditionKind, number>>>;
  /** The worst state among active layers, or null when none is on. */
  readonly worst: LayerConditionKind | null;
}

/** The aggregate the feed chip and the status bar show, from the same per-layer reading. */
export function feedHealth(
  catalog: Catalog | null,
  layers: Partial<Record<FeedId, LayerState>>,
  summaries: Partial<Record<FeedId, FeedSummary>>,
): FeedHealth {
  const counts: Partial<Record<LayerConditionKind, number>> = {};
  let worst: LayerConditionKind | null = null;
  let active = 0;
  for (const layer of catalog?.layers ?? []) {
    const { kind } = layerCondition(layer, layers[layer.feed], summaries[layer.feed]);
    if (!isActive(kind)) continue;
    active += 1;
    counts[kind] = (counts[kind] ?? 0) + 1;
    if (worst === null || SEVERITY[kind] > SEVERITY[worst]) worst = kind;
  }
  return { active, counts, worst };
}

const KIND_WORDS: Readonly<Partial<Record<LayerConditionKind, string>>> = {
  live: 'live',
  connecting: 'connecting',
  stale: 'stale',
  degraded: 'degraded',
  unavailable: 'unavailable',
};

/** "2 live, 1 stale" — every active state, worst last so the sentence ends on the news. */
export function describeHealth(health: FeedHealth): string {
  if (health.active === 0) return 'no feeds on';
  return (['live', 'connecting', 'stale', 'degraded', 'unavailable'] as const)
    .filter((kind) => (health.counts[kind] ?? 0) > 0)
    .map((kind) => `${health.counts[kind]} ${KIND_WORDS[kind]}`)
    .join(', ');
}
