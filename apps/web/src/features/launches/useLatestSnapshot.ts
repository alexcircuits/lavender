import type { FeedId, FeedSnapshot } from '../../contracts';
import { useFeeds, useRuntime } from '../../runtime';
import type { FeedSummary } from '../../state/stores';

export interface LatestSnapshot {
  /** The feed controller's latest snapshot, or undefined while the layer is off or loading. */
  readonly snapshot: FeedSnapshot | undefined;
  readonly summary: FeedSummary | undefined;
}

/**
 * The latest snapshot of a feed, for lists. Entity data stays in the feed controller (never
 * in a store); the feed's summary changes with every snapshot it receives, so subscribing to
 * it re-renders the caller exactly when there is a new snapshot to read.
 */
export function useLatestSnapshot(feed: FeedId): LatestSnapshot {
  const runtime = useRuntime();
  const summary = useFeeds((s) => s.summaries[feed]);
  return { snapshot: runtime.feeds.latest(feed), summary };
}
