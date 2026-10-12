import { useEffect, useState } from 'react';
import type { FeedId, LayerDescriptor } from '../../contracts';
import { formatCount } from '../../design';
import { useRuntime } from '../../runtime';
import type { FeedsState } from '../../state/stores';
import { itemNoun } from './families';

/** At most one announcement per layer in this window (design-system §13). */
export const ANNOUNCE_THROTTLE_MS = 10_000;
/** The region is emptied after this, so the same sentence can be announced again later. */
const CLEAR_AFTER_MS = 5_000;

/** Sentences for layers whose first data just arrived, honouring the per-layer throttle. */
function firstDataLines(
  layers: readonly LayerDescriptor[],
  state: FeedsState,
  previous: FeedsState,
  lastAnnounced: Map<FeedId, number>,
  at: number,
): string[] {
  const lines: string[] = [];
  for (const layer of layers) {
    const summary = state.summaries[layer.feed];
    if (summary?.phase !== 'ready' || previous.summaries[layer.feed]?.phase === 'ready') continue;
    const last = lastAnnounced.get(layer.feed);
    if (last !== undefined && at - last < ANNOUNCE_THROTTLE_MS) continue;
    lastAnnounced.set(layer.feed, at);
    lines.push(
      `${layer.name}: ${formatCount(summary.count)} ${itemNoun(layer.feed, summary.count)}`,
    );
  }
  return lines;
}

/**
 * A polite live region that says when a layer's first data arrives: "Earthquakes: 43 events".
 * It listens to the feeds store outside React's render path, so steady updates cost nothing,
 * and announces only the step from connecting (or failed) to data.
 */
export function LayerAnnouncer() {
  const runtime = useRuntime();
  const [message, setMessage] = useState('');

  useEffect(() => {
    const { feeds, system } = runtime.stores;
    const lastAnnounced = new Map<FeedId, number>();
    let clearTimer: ReturnType<typeof setTimeout> | undefined;
    const unsubscribe = feeds.subscribe((state, previous) => {
      if (state.summaries === previous.summaries) return;
      // A store listener must never throw: it would break the update for every other listener.
      try {
        const layers = system.getState().catalog?.layers ?? [];
        const lines = firstDataLines(layers, state, previous, lastAnnounced, Date.now());
        if (lines.length === 0) return;
        setMessage(lines.join('. '));
        clearTimeout(clearTimer);
        clearTimer = setTimeout(() => setMessage(''), CLEAR_AFTER_MS);
      } catch (error) {
        console.error('Layer announcement failed', error);
      }
    });
    return () => {
      unsubscribe();
      clearTimeout(clearTimer);
    };
  }, [runtime]);

  return (
    <div role="status" aria-live="polite" aria-label="Layer updates" className="visually-hidden">
      {message}
    </div>
  );
}
