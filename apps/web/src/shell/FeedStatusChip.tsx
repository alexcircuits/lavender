import { Button, type FeedStatus as PillStatus, Skeleton, StatusPill } from '../design';
import { useFeeds, useSystem, useUi, useWorkspace } from '../runtime';
import { describeHealth, feedHealth } from './feedHealth';
import styles from './shell.module.css';

const PILL: Readonly<Record<string, PillStatus>> = {
  live: 'live',
  stale: 'stale',
  degraded: 'degraded',
  unavailable: 'unavailable',
};

/**
 * Aggregate feed health, top right (design-system §6). Shows the worst state among the layers
 * that are on, with how many are in it; opens the Sources panel.
 */
export function FeedStatusChip() {
  const catalog = useSystem((s) => s.catalog);
  const layers = useWorkspace((s) => s.layers);
  const summaries = useFeeds((s) => s.summaries);
  const setSidePanel = useUi((s) => s.setSidePanel);

  if (!catalog) return <Skeleton width="96px" height="32px" shape="pill" />;

  const health = feedHealth(catalog, layers, summaries);
  const pill = health.worst ? PILL[health.worst] : undefined;
  const count = health.worst ? (health.counts[health.worst] ?? 0) : 0;

  return (
    <Button variant="secondary" className={styles.feedChip} onClick={() => setSidePanel('sources')}>
      <span className={styles.feedChipInner}>
        Feeds
        {pill ? (
          <StatusPill status={pill} detail={String(count)} aria-hidden />
        ) : (
          <span className={styles.feedChipQuiet} aria-hidden>
            {health.worst === 'connecting' ? 'Connecting…' : 'None on'}
          </span>
        )}
        <span className="visually-hidden">: {describeHealth(health)}. Show sources</span>
      </span>
    </Button>
  );
}
