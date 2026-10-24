/**
 * How layers are grouped, named and drawn in the explorer (design-system §4, §6).
 */
import type { FeedId, LayerFamily } from '../../contracts';
import {
  AirplaneIcon,
  BoatIcon,
  FireIcon,
  type Icon,
  NavigationArrowIcon,
  PathIcon,
  PlanetIcon,
  RocketLaunchIcon,
  WaveformIcon,
} from '../../design';
import { lookup } from '../../lib/lookup';

/** Explorer group order. Only families present in the catalog are shown. */
export const FAMILY_ORDER: readonly LayerFamily[] = [
  'air',
  'maritime',
  'space',
  'hazards',
  'weather',
  'infrastructure',
  'mobility',
  'sensors',
];

export const FAMILY_LABEL: Readonly<Record<LayerFamily, string>> = {
  air: 'Air',
  maritime: 'Maritime',
  space: 'Space',
  hazards: 'Hazards',
  weather: 'Weather',
  infrastructure: 'Infrastructure',
  mobility: 'Mobility',
  sensors: 'Sensors',
};

/**
 * The validated map colour each family draws with (§4: three hues plus neutrals; a fourth hue
 * fails all-pairs, so weather joins hazards and the context layers share the neutral).
 */
export type Swatch = 'air' | 'maritime' | 'space' | 'hazard' | 'context';

export const FAMILY_SWATCH: Readonly<Record<LayerFamily, Swatch>> = {
  air: 'air',
  maritime: 'maritime',
  space: 'space',
  hazards: 'hazard',
  weather: 'hazard',
  infrastructure: 'context',
  mobility: 'context',
  sensors: 'context',
};

export const FEED_GLYPH: Readonly<Record<FeedId, Icon>> = {
  aircraft: AirplaneIcon,
  earthquakes: WaveformIcon,
  satellites: PlanetIcon,
  cyclones: WaveformIcon,
  fires: FireIcon,
  launches: RocketLaunchIcon,
  vessels: BoatIcon,
  perimeters: FireIcon,
  bikeshare: PathIcon,
  transit: NavigationArrowIcon,
};

const NOUNS: Readonly<Record<FeedId, readonly [string, string]>> = {
  aircraft: ['contact', 'contacts'],
  earthquakes: ['event', 'events'],
  satellites: ['satellite', 'satellites'],
  cyclones: ['storm', 'storms'],
  fires: ['fire', 'fires'],
  launches: ['launch', 'launches'],
  vessels: ['vessel', 'vessels'],
  perimeters: ['perimeter', 'perimeters'],
  bikeshare: ['station', 'stations'],
  transit: ['vehicle', 'vehicles'],
};

/** "contact" / "contacts", the word a count of this feed's items is read with. */
export function itemNoun(feed: FeedId, count: number): string {
  const [one, many] = lookup(NOUNS, feed, ['item', 'items'] as const);
  return count === 1 ? one : many;
}

// The catalog comes from the server, which may be newer than this client: a family or feed
// it does not know still renders, with a neutral fallback, instead of throwing.

export function familyLabel(family: string): string {
  return lookup(FAMILY_LABEL, family, family.charAt(0).toUpperCase() + family.slice(1));
}

export function familySwatch(family: string): Swatch {
  return lookup(FAMILY_SWATCH, family, 'context');
}

export function feedGlyph(feed: string): Icon | undefined {
  return lookup<Icon | undefined>(FEED_GLYPH, feed, undefined);
}
