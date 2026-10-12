import type { FeedFilters, LayerDescriptor } from '../../contracts';
import { SegmentedControl, Toggle } from '../../design';
import { useWorkspace } from '../../runtime';
import {
  MAGNITUDE_OPTIONS,
  magnitudeValue,
  SATELLITE_GROUPS,
  selectedGroups,
  withGroup,
  withMilitaryOnly,
  withMinMagnitude,
} from './filters';
import styles from './layers.module.css';

const NO_FILTERS: FeedFilters = {};
const MAGNITUDE_SEGMENTS = MAGNITUDE_OPTIONS.map(({ value, label }) => ({ value, label }));

export interface LayerOptionsProps {
  readonly layer: LayerDescriptor;
}

/** The disclosure under a layer row: the filters that layer's feed understands. */
export function LayerOptions({ layer }: LayerOptionsProps) {
  const { feed } = layer;
  const filters = useWorkspace((s) => s.layers[feed]?.filters) ?? NO_FILTERS;
  const setFilters = useWorkspace((s) => s.setFilters);

  switch (feed) {
    case 'earthquakes':
      return (
        <div className={styles.optionStack}>
          <span className={styles.optionLabel} aria-hidden>
            Minimum magnitude
          </span>
          <SegmentedControl
            aria-label="Minimum magnitude"
            size="sm"
            options={MAGNITUDE_SEGMENTS}
            value={magnitudeValue(filters)}
            onValueChange={(value) => setFilters(feed, withMinMagnitude(filters, value))}
          />
        </div>
      );
    case 'aircraft':
      return (
        <div className={styles.optionRow}>
          <span>Military only</span>
          <Toggle
            aria-label="Military only"
            checked={filters.militaryOnly ?? false}
            onCheckedChange={(on) => setFilters(feed, withMilitaryOnly(filters, on))}
          />
        </div>
      );
    case 'satellites': {
      const shown = new Set(selectedGroups(filters));
      return (
        <fieldset className={styles.optionStack}>
          <legend className={styles.optionLabel}>Groups</legend>
          {SATELLITE_GROUPS.map((group) => (
            <div key={group.id} className={styles.optionRow}>
              <span>{group.label}</span>
              <Toggle
                aria-label={group.label}
                checked={shown.has(group.id)}
                onCheckedChange={(on) => setFilters(feed, withGroup(filters, group.id, on))}
              />
            </div>
          ))}
        </fieldset>
      );
    }
  }
}
