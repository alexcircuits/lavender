/**
 * Immutable edits of a layer's `FeedFilters`. An option at its default is removed rather than
 * stored, so "all" never travels to the server as a filter.
 */
import type { FeedFilters } from '../../contracts';

export const MAGNITUDE_OPTIONS = [
  { value: 'all', label: 'All', min: null },
  { value: '2.5', label: '2.5+', min: 2.5 },
  { value: '4.5', label: '4.5+', min: 4.5 },
  { value: '6', label: '6+', min: 6 },
] as const;

export type MagnitudeValue = (typeof MAGNITUDE_OPTIONS)[number]['value'];

export const SATELLITE_GROUPS = [
  { id: 'stations', label: 'Space stations' },
  { id: 'visual', label: 'Brightest' },
  { id: 'weather', label: 'Weather' },
  { id: 'gps-ops', label: 'GPS' },
  { id: 'galileo', label: 'Galileo' },
  { id: 'science', label: 'Science' },
] as const;

const ALL_GROUPS: readonly string[] = SATELLITE_GROUPS.map((g) => g.id);

export function magnitudeValue(filters: FeedFilters): MagnitudeValue {
  const match = MAGNITUDE_OPTIONS.find((o) => o.min === (filters.minMagnitude ?? null));
  return match?.value ?? 'all';
}

export function withMinMagnitude(filters: FeedFilters, value: MagnitudeValue): FeedFilters {
  const { minMagnitude: _previous, ...rest } = filters;
  const min = MAGNITUDE_OPTIONS.find((o) => o.value === value)?.min ?? null;
  return min === null ? rest : { ...rest, minMagnitude: min };
}

export function withMilitaryOnly(filters: FeedFilters, on: boolean): FeedFilters {
  const { militaryOnly: _previous, ...rest } = filters;
  return on ? { ...rest, militaryOnly: true } : rest;
}

/** The groups shown: no `groups` filter means every group. */
export function selectedGroups(filters: FeedFilters): readonly string[] {
  return filters.groups ?? ALL_GROUPS;
}

export function withGroup(filters: FeedFilters, group: string, on: boolean): FeedFilters {
  const { groups: _previous, ...rest } = filters;
  const current = new Set(selectedGroups(filters));
  if (on) current.add(group);
  else current.delete(group);
  const next = ALL_GROUPS.filter((g) => current.has(g));
  const extra = [...current].filter((g) => !ALL_GROUPS.includes(g));
  const groups = [...next, ...extra];
  return next.length === ALL_GROUPS.length && extra.length === 0 ? rest : { ...rest, groups };
}
