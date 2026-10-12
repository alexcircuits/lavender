/**
 * One honest reading of a layer's state (design-system §11), shared by the layer row, the feed
 * status chip and the status bar so they never disagree.
 */
import type { LayerDescriptor } from '../../contracts';
import { formatAge, type FeedStatus as PillStatus } from '../../design';
import type { FeedSummary, LayerState } from '../../state/stores';

export type LayerConditionKind =
  | 'unconfigured'
  | 'licence'
  | 'off'
  | 'connecting'
  | 'live'
  | 'stale'
  | 'degraded'
  | 'unavailable';

export interface LayerCondition {
  readonly kind: LayerConditionKind;
  /** The status pill to show, or null when none is needed (live, off, connecting). */
  readonly pill: PillStatus | null;
  /** Qualifier after the pill's word, e.g. "4 min" → "Stale 4 min". */
  readonly pillDetail: string | null;
  /** A sentence explaining why, when there is something to explain. */
  readonly reason: string | null;
}

const condition = (
  kind: LayerConditionKind,
  pill: PillStatus | null = null,
  reason: string | null = null,
  pillDetail: string | null = null,
): LayerCondition => ({ kind, pill, pillDetail, reason });

/** Names the missing configuration: "Needs FIRMS_MAP_KEY". */
export function missingReason(missing: readonly string[]): string {
  return missing.length > 0 ? `Needs ${missing.join(', ')}` : 'Needs configuration';
}

export const LICENCE_REASON = 'Off under the commercial licence policy';

export function layerCondition(
  layer: LayerDescriptor,
  state: LayerState | undefined,
  summary: FeedSummary | undefined,
): LayerCondition {
  const { availability } = layer;
  if (availability.state === 'unconfigured') {
    return condition('unconfigured', 'not-configured', missingReason(availability.missing));
  }
  if (availability.state === 'disabledByLicence') {
    return condition('licence', 'licence', LICENCE_REASON);
  }
  if (!state?.enabled) return condition('off');
  if (!summary || summary.phase === 'connecting') return condition('connecting');
  if (summary.phase === 'error') {
    return condition('unavailable', 'unavailable', summary.error?.message ?? null);
  }
  const partial = summary.coverage?.kind === 'partial' ? summary.coverage.reason : null;
  const freshness = summary.freshness;
  switch (freshness?.state) {
    case 'stale':
      return condition('stale', 'stale', partial, formatAge(freshness.ageS));
    case 'degraded':
      return condition('degraded', 'degraded', freshness.reason);
    case 'unavailable':
      return condition('unavailable', 'unavailable', freshness.reason);
    default:
      return condition('live', null, partial);
  }
}

/** Worst first: what the aggregate chip shows when layers disagree. */
export const SEVERITY: Readonly<Record<LayerConditionKind, number>> = {
  unavailable: 5,
  degraded: 4,
  stale: 3,
  connecting: 2,
  live: 1,
  off: 0,
  unconfigured: 0,
  licence: 0,
};

/** Whether the layer is turned on and backed by a configured, licensed source. */
export const isActive = (kind: LayerConditionKind) => SEVERITY[kind] > 0;
