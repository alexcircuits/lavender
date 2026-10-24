import { useEffect, useState } from 'react';
import { Badge, Button, CrosshairIcon, formatAge } from '../design';
import type { TrackingState } from '../globe';
import { useNow } from '../lib/useNow';
import { useGlobe } from '../runtime';
import styles from './shell.module.css';

/**
 * Floating status chip displayed while following an entity (ENG-977).
 *
 * Shows:
 * - "Following [ID] · Esc to stop"
 * - Badge "Stale (estimating)" when updates stop > 30s
 * - Badge "Lost (last seen [X] ago)" when updates stop > 60s
 * - Click on chip or press Esc stops following.
 */
export function FollowingChip() {
  const globe = useGlobe();
  const [followState, setFollowState] = useState<TrackingState | null>(
    () => globe?.getFollowState() ?? null,
  );
  const nowMs = useNow();

  useEffect(() => {
    if (!globe) return;
    setFollowState(globe.getFollowState());
    return globe.onFollowChange((state) => {
      setFollowState(state.status === 'released' ? null : state);
    });
  }, [globe]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && followState && followState.status !== 'released') {
        globe?.stopFollowing();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [globe, followState]);

  if (!followState || followState.status === 'released' || !followState.target) {
    return null;
  }

  const ageSec = followState.lastUpdateMs
    ? Math.max(0, Math.round((nowMs - followState.lastUpdateMs) / 1000))
    : 0;

  return (
    <Button
      variant="secondary"
      className={styles.followingChip}
      onClick={() => globe?.stopFollowing()}
      title="Click or press Esc to stop following"
    >
      <span className={styles.followingChipInner}>
        <CrosshairIcon size={14} aria-hidden />
        <span>Following {followState.target.id} · Esc to stop</span>
        {followState.status === 'stale' ? <Badge tone="accent">Stale (estimating)</Badge> : null}
        {followState.status === 'lost' ? (
          <Badge tone="neutral">Lost (last seen {formatAge(ageSec)} ago)</Badge>
        ) : null}
      </span>
    </Button>
  );
}
