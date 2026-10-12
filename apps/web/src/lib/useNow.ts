/**
 * One shared one-second clock for ages and freshness. Every subscriber re-renders on the same
 * tick, and the interval only runs while something is subscribed.
 */
import { useSyncExternalStore } from 'react';

const TICK_MS = 1000;
const listeners = new Set<() => void>();
let timer: ReturnType<typeof setInterval> | null = null;
let now = Date.now();

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  if (timer === null) {
    now = Date.now();
    timer = setInterval(() => {
      now = Date.now();
      for (const l of listeners) l();
    }, TICK_MS);
  }
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0 && timer !== null) {
      clearInterval(timer);
      timer = null;
    }
  };
}

const snapshot = () => now;

/** The current time in milliseconds, updated once a second while mounted. */
export function useNow(): number {
  return useSyncExternalStore(subscribe, snapshot, snapshot);
}
