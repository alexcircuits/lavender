/** Entity references carried as primitive `id`s, and recognising them in pick results. */
import type { FeedId } from '../contracts';
import type { EntityRef } from './index';

const FEEDS: ReadonlySet<string> = new Set<FeedId>([
  'aircraft',
  'earthquakes',
  'satellites',
  'cyclones',
]);

export function isEntityRef(value: unknown): value is EntityRef {
  if (typeof value !== 'object' || value === null) return false;
  const candidate = value as { feed?: unknown; id?: unknown };
  return (
    typeof candidate.feed === 'string' &&
    FEEDS.has(candidate.feed) &&
    typeof candidate.id === 'string' &&
    candidate.id.length > 0
  );
}

/** The entity a `scene.pick` result refers to, or null (globe, imagery, nothing). */
export function entityFromPick(picked: unknown): EntityRef | null {
  if (typeof picked !== 'object' || picked === null) return null;
  const id = (picked as { id?: unknown }).id;
  return isEntityRef(id) ? { feed: id.feed, id: id.id } : null;
}

export function sameEntity(a: EntityRef | null, b: EntityRef | null): boolean {
  return a === b || (a !== null && b !== null && a.feed === b.feed && a.id === b.id);
}
