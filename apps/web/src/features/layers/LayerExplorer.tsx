import { type ReactNode, useMemo, useRef, useState } from 'react';
import type { Catalog, LayerDescriptor, LayerFamily } from '../../contracts';
import {
  Button,
  EmptyState,
  IconButton,
  List,
  MagnifyingGlassIcon,
  Panel,
  SearchField,
  SegmentedControl,
  StackIcon,
  XIcon,
} from '../../design';
import { useUi, useWorkspace } from '../../runtime';
import { BoundariesRow } from './BoundariesRow';
import { FAMILY_ORDER, familyLabel, familySwatch, type Swatch } from './families';
import { LayerRow } from './LayerRow';
import styles from './layers.module.css';

type LayerCategoryTab = 'all' | Swatch;

const LAYER_CATEGORY_TABS: readonly { value: LayerCategoryTab; label: string }[] = [
  { value: 'all', label: 'All families' },
  { value: 'air', label: 'Air' },
  { value: 'maritime', label: 'Sea' },
  { value: 'space', label: 'Space' },
  { value: 'hazard', label: 'Hazards' },
  { value: 'context', label: 'Context' },
];

interface LayerGroup {
  readonly family: LayerFamily;
  readonly label: string;
  readonly layers: readonly LayerDescriptor[];
}

/**
 * Layers grouped by family in the explorer's order, narrowed by the filter text. Families this
 * client does not know (a newer server) follow the known ones rather than disappearing.
 */
export function groupLayers(layers: readonly LayerDescriptor[], filter: string): LayerGroup[] {
  const q = filter.trim().toLowerCase();
  const matches = (layer: LayerDescriptor) =>
    q === '' ||
    [layer.name, layer.description, familyLabel(layer.family)].some((text) =>
      text.toLowerCase().includes(q),
    );
  const known = new Set<string>(FAMILY_ORDER);
  const extra = [...new Set(layers.map((l) => l.family))].filter((f) => !known.has(f));
  return [...FAMILY_ORDER, ...extra].flatMap((family) => {
    const inFamily = layers.filter((l) => l.family === family && matches(l));
    return inFamily.length > 0 ? [{ family, label: familyLabel(family), layers: inFamily }] : [];
  });
}

function LayersFooterActions({ layers }: { layers: readonly LayerDescriptor[] }) {
  const layerStates = useWorkspace((s) => s.layers);
  const setLayerEnabled = useWorkspace((s) => s.setLayerEnabled);
  const activeFeeds = useMemo(
    () =>
      layers
        .filter((l) => l.availability.state === 'available' && layerStates[l.feed]?.enabled)
        .map((l) => l.feed),
    [layers, layerStates],
  );
  const on = activeFeeds.length;

  return (
    <>
      <span className={styles.footnote}>
        {on} of {layers.length} layers on
      </span>
      {on > 0 ? (
        <Button
          variant="ghost"
          size="sm"
          onClick={() => {
            for (const feed of activeFeeds) {
              setLayerEnabled(feed, false);
            }
          }}
        >
          Turn all off
        </Button>
      ) : null}
    </>
  );
}

export interface LayerExplorerProps {
  readonly catalog: Catalog;
  readonly className?: string | undefined;
}

/** The layer explorer island (design-system §6): grouped by family, one row per layer. */
export function LayerExplorer({ catalog, className }: LayerExplorerProps) {
  const [filter, setFilter] = useState('');
  const [categoryTab, setCategoryTab] = useState<LayerCategoryTab>('all');
  const filterRef = useRef<HTMLInputElement | null>(null);
  const setSidePanel = useUi((s) => s.setSidePanel);
  const groups = useMemo(() => groupLayers(catalog.layers, filter), [catalog, filter]);
  const sourceNames = useMemo(() => {
    const names = new Map(catalog.sources.map((s) => [s.id, s.name]));
    return new Map(
      catalog.layers.map((l) => [l.feed, l.sourceIds.map((id) => names.get(id) ?? id).join(', ')]),
    );
  }, [catalog]);

  const hasNaturalEarth = catalog.sources.some((s) => s.id === 'natural-earth');
  const q = filter.trim().toLowerCase();
  const matchesBoundaries =
    q === '' ||
    ['boundaries', 'borders', 'disputed', 'breakaway', 'maritime', 'natural earth'].some(
      (term) => term.includes(q) || q.includes(term),
    );

  const boundariesSection =
    hasNaturalEarth && matchesBoundaries ? (
      <section
        key="boundaries-context"
        className={styles.group}
        aria-labelledby="layer-group-boundaries"
        hidden={categoryTab !== 'all' && categoryTab !== 'context'}
      >
        <h3 id="layer-group-boundaries" className={styles.groupLabel}>
          Context
        </h3>
        <List aria-label="Context layers" dense>
          <BoundariesRow onOpenSources={() => setSidePanel('sources')} />
        </List>
      </section>
    ) : null;

  let body: ReactNode;
  if (catalog.layers.length === 0 && !boundariesSection) {
    body = (
      <EmptyState icon={StackIcon}>This server offers no layers yet. Check its sources.</EmptyState>
    );
  } else if (groups.length === 0 && !boundariesSection) {
    body = (
      <EmptyState
        icon={MagnifyingGlassIcon}
        action={
          <Button
            size="sm"
            variant="ghost"
            onClick={() => {
              setFilter('');
              filterRef.current?.focus();
            }}
          >
            Clear filter
          </Button>
        }
      >
        No layers match '{filter.trim()}'.
      </EmptyState>
    );
  } else {
    const layerSections = groups.map((group) => {
      const swatch = familySwatch(group.family);
      return (
        <section
          key={group.family}
          className={styles.group}
          aria-labelledby={`layer-group-${group.family}`}
          hidden={categoryTab !== 'all' && categoryTab !== swatch}
        >
          <h3 id={`layer-group-${group.family}`} className={styles.groupLabel}>
            {group.label}
          </h3>
          <List aria-label={`${group.label} layers`} dense>
            {group.layers.map((layer) => (
              <LayerRow
                key={layer.feed}
                layer={layer}
                sources={sourceNames.get(layer.feed) ?? ''}
              />
            ))}
          </List>
        </section>
      );
    });

    body = (
      <>
        {layerSections}
        {boundariesSection}
      </>
    );
  }

  return (
    <Panel
      title="Layers"
      dense
      className={className}
      actions={<IconButton label="Close layers" icon={XIcon} onClick={() => setSidePanel(null)} />}
      toolbar={
        <div className={styles.toolbarStack}>
          <SearchField
            ref={filterRef}
            label="Filter layers"
            placeholder="Filter layers"
            size="sm"
            value={filter}
            onValueChange={setFilter}
          />
          <SegmentedControl<LayerCategoryTab>
            aria-label="Layer family filter"
            options={LAYER_CATEGORY_TABS}
            value={categoryTab}
            onValueChange={setCategoryTab}
          />
        </div>
      }
      footer={
        <>
          <LayersFooterActions layers={catalog.layers} />
          <Button variant="ghost" size="sm" onClick={() => setSidePanel('sources')}>
            Sources
          </Button>
        </>
      }
    >
      {body}
    </Panel>
  );
}
