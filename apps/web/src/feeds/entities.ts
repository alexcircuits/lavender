/**
 * Engine-free helpers over snapshot items, shared by the feed controller, the globe
 * renderers and the inspector.
 */
import type {
  Aircraft,
  Cyclone,
  Earthquake,
  FeedId,
  FeedItems,
  SatelliteElements,
} from '../contracts';

export type FeedItem = Aircraft | Earthquake | SatelliteElements | Cyclone;

/** The stable id of an item within its feed. */
export function itemId(items: FeedItems, item: FeedItem): string {
  switch (items.kind) {
    case 'aircraft':
      return (item as Aircraft).hex;
    case 'earthquakes':
      return (item as Earthquake).id;
    case 'satellites':
      return String((item as SatelliteElements).noradId);
    case 'cyclones':
      return (item as Cyclone).id;
  }
}

export function itemsOf(items: FeedItems): readonly FeedItem[] {
  return items.items;
}

/** Finds an item by id in a snapshot's items. */
export function findItem(items: FeedItems, id: string): FeedItem | undefined {
  return itemsOf(items).find((it) => itemId(items, it) === id);
}

/** Feeds whose server-side filtering uses the viewport (satellites are global objects). */
export function usesViewport(feed: FeedId): boolean {
  return feed === 'aircraft' || feed === 'earthquakes';
}
