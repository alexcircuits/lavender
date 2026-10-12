import { useState } from 'react';
import { formatCount } from '../format';
import {
  AirplaneIcon,
  BoatIcon,
  CaretDownIcon,
  FireIcon,
  HurricaneIcon,
  type Icon,
  PathIcon,
  PlanetIcon,
  SlidersHorizontalIcon,
  WaveformIcon,
} from '../icons';
import { Button } from '../primitives/Button';
import { FreshnessMeter } from '../primitives/FreshnessMeter';
import { IconButton } from '../primitives/IconButton';
import { List, ListRow } from '../primitives/List';
import { Panel } from '../primitives/Panel';
import { SearchField } from '../primitives/SearchField';
import { StatusPill } from '../primitives/StatusPill';
import { Toggle } from '../primitives/Toggle';
import styles from './DesignReview.module.css';
import { type Family, LAYER_GROUPS, type ReviewLayer } from './data';

const GLYPHS: Readonly<Record<string, Icon>> = {
  aircraft: AirplaneIcon,
  vessels: BoatIcon,
  satellites: PlanetIcon,
  earthquakes: WaveformIcon,
  fires: FireIcon,
  cyclones: HurricaneIcon,
  cables: PathIcon,
};

export function FamilySwatch({ family, icon: Glyph }: { family: Family; icon?: Icon | undefined }) {
  return (
    <span className={styles.swatch} data-family={family} aria-hidden>
      {Glyph ? <Glyph size={12} weight="fill" /> : null}
    </span>
  );
}

function LayerRow({
  layer,
  on,
  onToggle,
}: {
  layer: ReviewLayer;
  on: boolean;
  onToggle: (on: boolean) => void;
}) {
  const source = layer.status === 'not-configured' ? 'Add a FIRMS key in Sources' : layer.source;
  return (
    <ListRow
      leading={<FamilySwatch family={layer.family} icon={GLYPHS[layer.id]} />}
      title={layer.name}
      description={
        // Status sits on the second line so names never truncate in the 320 px island.
        <span className={styles.rowStatus}>
          {layer.status === 'live' ? null : (
            <StatusPill
              size="sm"
              status={layer.status}
              {...(layer.statusDetail ? { detail: layer.statusDetail } : {})}
            />
          )}
          <span className={styles.rowSource}>{source}</span>
        </span>
      }
      meta={
        <>
          {layer.count !== null && on ? (
            <span className={styles.count}>{formatCount(layer.count)}</span>
          ) : null}
          {on && !layer.locked ? (
            <FreshnessMeter
              label={layer.name}
              ageSeconds={layer.status === 'unavailable' ? null : layer.ageSeconds}
              cadenceSeconds={layer.cadenceSeconds}
            />
          ) : null}
        </>
      }
      trailing={
        <Toggle
          aria-label={layer.name}
          checked={on}
          disabled={layer.locked ?? false}
          onCheckedChange={onToggle}
        />
      }
    />
  );
}

/** The layer explorer island (design-system §6): grouped by family, one row per layer. */
export function LayersIsland() {
  const [enabled, setEnabled] = useState<ReadonlySet<string>>(
    () =>
      new Set(
        LAYER_GROUPS.flatMap((g) => g.layers)
          .filter((l) => l.on)
          .map((l) => l.id),
      ),
  );
  const [filter, setFilter] = useState('');
  const query = filter.trim().toLowerCase();
  const total = LAYER_GROUPS.reduce((sum, group) => sum + group.layers.length, 0);

  const toggle = (id: string, on: boolean) =>
    setEnabled((current) => {
      const next = new Set(current);
      if (on) next.add(id);
      else next.delete(id);
      return next;
    });

  return (
    <Panel
      title="Layers"
      dense
      className={styles.layers}
      actions={<IconButton label="Layer options" icon={SlidersHorizontalIcon} />}
      toolbar={
        <SearchField
          label="Filter layers"
          placeholder="Filter layers"
          size="sm"
          shortcutHint="/"
          value={filter}
          onValueChange={setFilter}
        />
      }
      footer={
        <>
          <span className={styles.footnote}>
            {enabled.size} of {total} layers on
          </span>
          <Button variant="ghost" size="sm" iconEnd={CaretDownIcon}>
            Sources
          </Button>
        </>
      }
    >
      {LAYER_GROUPS.map((group) => {
        const layers = group.layers.filter((l) => l.name.toLowerCase().includes(query));
        if (layers.length === 0) return null;
        return (
          <section key={group.label} className={styles.group} aria-label={group.label}>
            <h3 className={styles.groupLabel}>{group.label}</h3>
            <List aria-label={`${group.label} layers`} dense>
              {layers.map((layer) => (
                <LayerRow
                  key={layer.id}
                  layer={layer}
                  on={enabled.has(layer.id)}
                  onToggle={(on) => toggle(layer.id, on)}
                />
              ))}
            </List>
          </section>
        );
      })}
    </Panel>
  );
}
