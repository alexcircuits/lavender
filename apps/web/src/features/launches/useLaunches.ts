import { useMemo } from 'react';
import type { Launch } from '../../contracts';
import { launchClock, wellFormedLaunches } from '../../lib/launches';
import { useNow } from '../../lib/useNow';
import type { FeedSummary } from '../../state/stores';
import { useLatestSnapshot } from './useLatestSnapshot';

export interface LatestLaunches {
  /** The latest snapshot's well-formed launches (a malformed record is skipped). */
  readonly launches: readonly Launch[];
  /** Whether any launches snapshot has arrived since the layer was turned on. */
  readonly hasSnapshot: boolean;
  readonly summary: FeedSummary | undefined;
}

const NONE: readonly Launch[] = [];

/** The latest launches, validated once per snapshot. */
export function useLaunches(): LatestLaunches {
  const { snapshot, summary } = useLatestSnapshot('launches');
  const launches = useMemo(
    () => (snapshot?.items.kind === 'launches' ? wellFormedLaunches(snapshot.items.items) : NONE),
    [snapshot],
  );
  return { launches, hasSnapshot: snapshot !== undefined, summary };
}

/** The launch clock (`launchClock`): the current minute, a value that changes once a minute. */
export function useLaunchClock(): number {
  return launchClock(useNow());
}
