import { type ReactNode, type RefObject, useLayoutEffect, useMemo, useRef } from 'react';
import type { Catalog, GeoPoint, LayerDescriptor } from '../../contracts';
import {
  ArrowSquareOutIcon,
  Button,
  CopyIcon,
  EmptyState,
  IconButton,
  InfoIcon,
  NavigationArrowIcon,
  Panel,
  showToast,
  WarningIcon,
  XIcon,
} from '../../design';
import type { EntityRef } from '../../globe';
import { formatDecimal } from '../../lib/coordinates';
import { noaaRecordUrl, usgsEventUrl } from '../../lib/safeUrl';
import { formatUtcTime, parseTime } from '../../lib/time';
import { useNow } from '../../lib/useNow';
import { useFeeds, useGlobe, useSystem, useWorkspace } from '../../runtime';
import type { FeedSummary } from '../../state/stores';
import { GLOBE_REGION_ID } from '../commands';
import { FamilySwatch, feedGlyph } from '../layers';
import { AircraftHistory } from './AircraftHistory';
import { CycloneForecast } from './CycloneForecast';
import { FactSheet } from './FactSheet';
import {
  aircraftFacts,
  cycloneFacts,
  earthquakeFacts,
  type Fact,
  type Inspected,
  inspect,
  positionFacts,
  provenanceFacts,
  satelliteFacts,
  satelliteGroupLabel,
} from './facts';
import styles from './inspector.module.css';
import { SatellitePasses } from './SatellitePasses';
import { useSatellitePosition } from './useSatellitePosition';

/** Copies a position to the clipboard and says what happened, in the action's own verb. */
export async function copyCoordinates(position: GeoPoint): Promise<void> {
  const text = formatDecimal(position);
  try {
    if (!navigator.clipboard) throw new Error('The clipboard is not available');
    await navigator.clipboard.writeText(text);
    showToast({ kind: 'success', title: 'Coordinates copied', description: text });
  } catch {
    showToast({
      kind: 'error',
      title: 'Coordinates not copied',
      description: `This browser did not allow access to the clipboard. The position is ${text}.`,
    });
  }
}

function sourceNames(ids: readonly string[], catalog: Catalog | null): string[] {
  const unique = [...new Set(ids)];
  return unique.map((id) => catalog?.sources.find((s) => s.id === id)?.name ?? id);
}

interface Sheet {
  readonly title: string;
  readonly summary: string;
  readonly facts: Fact[];
  readonly position: GeoPoint | null;
  readonly observedMs: number | null;
  readonly sourceIds: readonly string[];
  readonly recordUrl: string | null;
}

function sheetFor(
  view: Inspected,
  summary: FeedSummary | undefined,
  satellite: ReturnType<typeof useSatellitePosition>,
  nowMs: number,
): Sheet {
  const fetchedMs = parseTime(summary?.fetchedAt);
  const snapshotSources = summary?.sources ?? [];
  switch (view.kind) {
    case 'cyclones': {
      const storm = view.item;
      return {
        title: storm.name,
        summary: `${storm.classification} · ${storm.id}`,
        facts: cycloneFacts(storm),
        position: storm.position,
        observedMs: parseTime(storm.positionAt),
        sourceIds: snapshotSources,
        recordUrl: noaaRecordUrl(storm.advisoryUrl) ?? noaaRecordUrl(storm.outlookUrl),
      };
    }
    case 'aircraft': {
      const a = view.item;
      const detail = [a.typeCode, a.registration, a.isMilitary ? 'Military' : null].filter(Boolean);
      return {
        title: a.callsign?.trim() || a.hex,
        summary: detail.length > 0 ? detail.join(' · ') : 'Aircraft',
        facts: aircraftFacts(a),
        position: a.position,
        observedMs: fetchedMs === null ? null : fetchedMs - a.seenPosS * 1000,
        sourceIds: [a.source],
        recordUrl: null,
      };
    }
    case 'earthquakes': {
      const q = view.item;
      return {
        title: `M ${q.magnitude.toFixed(1)} earthquake`,
        summary: q.place?.trim() || 'Earthquake',
        facts: earthquakeFacts(q, nowMs),
        position: q.position,
        observedMs: parseTime(q.time) ?? parseTime(q.updated),
        sourceIds: snapshotSources,
        recordUrl: usgsEventUrl(q.url),
      };
    }
    case 'satellites': {
      const s = view.item;
      const point = satellite.position;
      const located: Fact[] = point
        ? [
            ...positionFacts(point).map((f) =>
              f.label === 'Position' ? { ...f, label: 'Sub-satellite point' } : f,
            ),
            ...(point.altM === undefined
              ? []
              : [{ label: 'Altitude', value: `${Math.round(point.altM / 1000)} km` }]),
            ...(satellite.atMs === null
              ? []
              : [{ label: 'Position time', value: formatUtcTime(satellite.atMs), mono: true }]),
          ]
        : [
            {
              label: 'Sub-satellite point',
              value: satellite.status === 'loading' ? 'Locating…' : 'Not available',
            },
          ];
      return {
        title: s.name,
        summary: `NORAD ${s.noradId} · ${satelliteGroupLabel(s.group)}`,
        facts: [...satelliteFacts(s), ...located],
        position: point,
        observedMs: parseTime(s.epoch),
        sourceIds: snapshotSources,
        recordUrl: null,
      };
    }
  }
}

/** Why the selected entity has no record to show, and what will change that. */
function emptyMessage(
  selection: EntityRef,
  name: string,
  canTurnOn: boolean,
  waiting: boolean,
): string {
  if (canTurnOn) return `${selection.id} is in ${name}, which is off. Turn it on to see it.`;
  if (waiting) return `Waiting for ${name} data…`;
  // Aircraft and earthquakes are fetched for the area the globe shows; satellites worldwide.
  if (selection.feed === 'aircraft' || selection.feed === 'earthquakes') {
    return `${selection.id} is not in the latest ${name} data for the area the globe shows.`;
  }
  return `${selection.id} is not in the latest ${name} data.`;
}

/** Keeps keyboard focus sensible when the inspector closes: back to whatever opened it. */
function useReturnFocus(ref: RefObject<HTMLElement | null>): void {
  useLayoutEffect(() => {
    const active = document.activeElement;
    const opener = active instanceof HTMLElement && active !== document.body ? active : null;
    return () => {
      const panel = ref.current;
      const focused = document.activeElement;
      if (!panel || !(focused instanceof Node) || !panel.contains(focused)) return;
      const target = opener?.isConnected ? opener : document.getElementById(GLOBE_REGION_ID);
      target?.focus();
    };
  }, [ref]);
}

interface PanelFrameProps {
  readonly title: string;
  readonly layer: LayerDescriptor | undefined;
  readonly summary?: string;
  readonly className?: string | undefined;
  readonly children: ReactNode;
}

function InspectorFrame({ title, layer, summary, className, children }: PanelFrameProps) {
  const ref = useRef<HTMLElement | null>(null);
  const select = useWorkspace((s) => s.select);
  useReturnFocus(ref);
  return (
    <Panel
      ref={ref}
      title={title}
      className={className}
      data-inspector=""
      actions={
        <IconButton
          label="Close inspector"
          icon={XIcon}
          shortcut="Esc"
          tooltipSide="bottom"
          onClick={() => select(null)}
        />
      }
      toolbar={
        summary ? (
          <p className={styles.summary}>
            {layer ? <FamilySwatch family={layer.family} icon={feedGlyph(layer.feed)} /> : null}
            <span className={styles.summaryText}>{summary}</span>
          </p>
        ) : null
      }
    >
      {children}
    </Panel>
  );
}

interface InspectorBodyProps {
  readonly selection: EntityRef;
  readonly className?: string | undefined;
}

function InspectorBody({ selection, className }: InspectorBodyProps) {
  const entity = useFeeds((s) => s.selectedEntity);
  const present = useFeeds((s) => s.selectedPresent);
  const lastSeen = useFeeds((s) => s.selectedLastSeen);
  const summary = useFeeds((s) => s.summaries[selection.feed]);
  const catalog = useSystem((s) => s.catalog);
  const enabled = useWorkspace((s) => s.layers[selection.feed]?.enabled ?? false);
  const setLayerEnabled = useWorkspace((s) => s.setLayerEnabled);
  const globe = useGlobe();
  const nowMs = useNow();
  const layer = catalog?.layers.find((l) => l.feed === selection.feed);
  const view = useMemo(
    () => inspect(selection.feed, selection.id, entity),
    [selection.feed, selection.id, entity],
  );
  const noradId = selection.feed === 'satellites' ? Number(selection.id) : Number.NaN;
  const satellite = useSatellitePosition(Number.isInteger(noradId) && noradId > 0 ? noradId : null);

  if (!view) {
    const name = layer?.name ?? selection.feed;
    const canTurnOn = !enabled && layer?.availability.state === 'available';
    const waiting = enabled && (!summary || summary.phase === 'connecting');
    return (
      <InspectorFrame title={selection.id} layer={layer} className={className}>
        <EmptyState
          icon={InfoIcon}
          action={
            canTurnOn ? (
              <Button size="sm" onClick={() => setLayerEnabled(selection.feed, true)}>
                Turn on {name}
              </Button>
            ) : null
          }
        >
          {emptyMessage(selection, name, canTurnOn, waiting)}
        </EmptyState>
      </InspectorFrame>
    );
  }

  const sheet = sheetFor(view, summary, satellite, nowMs);
  const lastSeenMs = parseTime(lastSeen);
  const provenance = provenanceFacts({
    sources: sourceNames(
      sheet.sourceIds.length > 0 ? sheet.sourceIds : (layer?.sourceIds ?? []),
      catalog,
    ),
    observedMs: sheet.observedMs,
    fetchedMs: parseTime(summary?.fetchedAt),
    nowMs,
  });
  const position = sheet.position;

  return (
    <InspectorFrame title={sheet.title} summary={sheet.summary} layer={layer} className={className}>
      {present ? null : (
        <p className={styles.gone} role="status">
          <WarningIcon size={14} weight="fill" aria-hidden />
          {lastSeenMs === null
            ? 'No longer in feed'
            : `No longer in feed · last seen ${formatUtcTime(lastSeenMs)}`}
        </p>
      )}
      <FactSheet facts={sheet.facts} label="Facts" />
      <h3 className={styles.subhead}>Provenance</h3>
      <FactSheet facts={provenance} label="Provenance" />
      {view.kind === 'aircraft' ? (
        <AircraftHistory key={view.item.hex} hex={view.item.hex} />
      ) : null}
      {view.kind === 'satellites' ? (
        <SatellitePasses key={view.item.noradId} noradId={view.item.noradId} />
      ) : null}
      {view.kind === 'cyclones' ? <CycloneForecast storm={view.item} /> : null}
      <div className={styles.actions}>
        <Button
          variant="primary"
          icon={NavigationArrowIcon}
          disabled={!globe || !position}
          onClick={() => position && globe?.flyTo({ kind: 'point', position })}
        >
          Fly to
        </Button>
        <Button
          variant="ghost"
          icon={CopyIcon}
          disabled={!position}
          onClick={() => position && void copyCoordinates(position)}
        >
          Copy coordinates
        </Button>
        {sheet.recordUrl ? (
          <a
            className={styles.linkButton}
            href={sheet.recordUrl}
            target="_blank"
            rel="noopener noreferrer"
          >
            <ArrowSquareOutIcon size={16} weight="bold" aria-hidden className={styles.linkIcon} />
            Open source record
          </a>
        ) : null}
      </div>
    </InspectorFrame>
  );
}

export interface InspectorProps {
  readonly className?: string | undefined;
}

/**
 * The context inspector (design-system §6): the selected entity's facts, provenance and
 * actions. It appears on selection and never moves the camera on its own.
 */
export function Inspector({ className }: InspectorProps) {
  const selection = useWorkspace((s) => s.selection);
  if (!selection) return null;
  // A new selection is a new sheet: per-entity state (satellite position) starts fresh.
  return (
    <InspectorBody
      key={`${selection.feed}:${selection.id}`}
      selection={selection}
      className={className}
    />
  );
}
