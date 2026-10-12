import { type KeyboardEvent, type MouseEvent, memo, useId, useState } from 'react';
import type { LayerDescriptor } from '../../contracts';
import {
  FreshnessMeter,
  formatCount,
  IconButton,
  ListRow,
  SlidersHorizontalIcon,
  StatusPill,
  Toggle,
} from '../../design';
import { parseTime, secondsBetween } from '../../lib/time';
import { useNow } from '../../lib/useNow';
import { useFeeds, useUi, useWorkspace } from '../../runtime';
import type { FeedSummary } from '../../state/stores';
import { FamilySwatch } from './FamilySwatch';
import { feedGlyph, itemNoun } from './families';
import { LayerOptions } from './LayerOptions';
import styles from './layers.module.css';
import { type LayerCondition, layerCondition } from './status';

export interface LayerRowProps {
  readonly layer: LayerDescriptor;
  /** Provider names for the second line, e.g. "USGS". */
  readonly sources: string;
}

/** Only this child ticks with the clock, so the row itself re-renders on its own feed only. */
function RowFreshness({ layer, fetchedAt }: { layer: LayerDescriptor; fetchedAt: string | null }) {
  const now = useNow();
  const fetched = parseTime(fetchedAt);
  return (
    <FreshnessMeter
      label={layer.name}
      ageSeconds={fetched === null ? null : secondsBetween(fetched, now)}
      cadenceSeconds={layer.cadenceS}
    />
  );
}

/** The List's row keys, which inside the options belong to the controls, not the list. */
const LIST_KEYS = new Set(['j', 'k', 'ArrowDown', 'ArrowUp', 'Home', 'End']);

/**
 * Inside the options disclosure, j/k and the arrows must not reach the List, which would move
 * focus to the first row (the disclosure is not a row). Radio groups still get their arrows:
 * this runs after the control's own handler, on the way up.
 */
function keepListKeys(event: KeyboardEvent<HTMLFieldSetElement>): void {
  if (LIST_KEYS.has(event.key)) event.stopPropagation();
}

function hasData(summary: FeedSummary | undefined): summary is FeedSummary {
  if (!summary) return false;
  return summary.phase === 'ready' || (summary.phase === 'error' && summary.fetchedAt !== null);
}

interface DescriptionProps {
  readonly condition: LayerCondition;
  readonly sources: string;
  readonly reasonId: string;
  readonly onOpenSources: () => void;
}

function RowDescription({ condition, sources, reasonId, onOpenSources }: DescriptionProps) {
  if (condition.kind === 'connecting') {
    return (
      <span className={styles.connecting}>
        <span>Connecting…</span>
        <span className={styles.progress} aria-hidden />
      </span>
    );
  }
  const problem = condition.kind === 'unavailable' || condition.kind === 'degraded';
  return (
    <span className={styles.rowStatus}>
      {condition.pill ? (
        <StatusPill
          size="sm"
          status={condition.pill}
          {...(condition.pillDetail ? { detail: condition.pillDetail } : {})}
        />
      ) : null}
      <span id={reasonId} className={problem ? styles.reason : styles.rowText}>
        {condition.reason ?? sources}
      </span>
      {condition.kind === 'unconfigured' ? (
        <button
          type="button"
          className={styles.inlineLink}
          onClick={(event: MouseEvent<HTMLButtonElement>) => {
            // Keeps the row's own click (toggle) from running.
            event.preventDefault();
            onOpenSources();
          }}
        >
          Set up in Sources
        </button>
      ) : null}
    </span>
  );
}

/**
 * One layer: swatch, name, status line, count, freshness, options and the on/off switch.
 * Subscribes to its own feed summary and layer state only, so a feed update re-renders the
 * row that shows it and nothing else (frontend.md §3).
 */
export const LayerRow = memo(function LayerRow({ layer, sources }: LayerRowProps) {
  const { feed } = layer;
  const state = useWorkspace((s) => s.layers[feed]);
  const summary = useFeeds((s) => s.summaries[feed]);
  const setLayerEnabled = useWorkspace((s) => s.setLayerEnabled);
  const setSidePanel = useUi((s) => s.setSidePanel);
  const [optionsOpen, setOptionsOpen] = useState(false);
  const reasonId = useId();
  const optionsId = useId();

  const enabled = state?.enabled ?? false;
  const available = layer.availability.state === 'available';
  const condition = layerCondition(layer, state, summary);
  const toggle = () => setLayerEnabled(feed, !enabled);

  const meta =
    enabled && hasData(summary) ? (
      <>
        <span className={styles.count}>
          {formatCount(summary.count)}
          <span className="visually-hidden"> {itemNoun(feed, summary.count)}</span>
        </span>
        <RowFreshness layer={layer} fetchedAt={summary.fetchedAt} />
      </>
    ) : null;

  return (
    <>
      <ListRow
        data-feed={feed}
        data-condition={condition.kind}
        leading={<FamilySwatch family={layer.family} icon={feedGlyph(feed)} />}
        title={layer.name}
        description={
          <RowDescription
            condition={condition}
            sources={sources}
            reasonId={reasonId}
            onOpenSources={() => setSidePanel('sources')}
          />
        }
        meta={meta}
        trailing={
          <>
            {available ? (
              <IconButton
                label={`${layer.name} options`}
                icon={SlidersHorizontalIcon}
                size="sm"
                aria-expanded={optionsOpen}
                {...(optionsOpen ? { 'aria-controls': optionsId } : {})}
                onClick={() => setOptionsOpen((open) => !open)}
              />
            ) : null}
            <Toggle
              aria-label={layer.name}
              checked={enabled}
              disabled={!available}
              onCheckedChange={(on) => setLayerEnabled(feed, on)}
              {...(condition.reason ? { 'aria-describedby': reasonId } : {})}
            />
          </>
        }
        {...(available ? { onSelect: toggle } : {})}
      />
      {available && optionsOpen ? (
        <li id={optionsId} className={styles.options}>
          <fieldset
            aria-label={`${layer.name} options`}
            className={styles.optionsGroup}
            onKeyDown={keepListKeys}
          >
            <LayerOptions layer={layer} />
          </fieldset>
        </li>
      ) : null}
    </>
  );
});
