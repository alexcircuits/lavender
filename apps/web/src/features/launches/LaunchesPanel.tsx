import { memo, useCallback, useId, useMemo } from 'react';
import type { Catalog, Launch, LayerDescriptor } from '../../contracts';
import {
  Button,
  EmptyState,
  ErrorState,
  formatCount,
  IconButton,
  InfoIcon,
  List,
  ListRow,
  Panel,
  RocketLaunchIcon,
  Skeleton,
  StackIcon,
  StatusPill,
  XIcon,
} from '../../design';
import { awaitingOutcome, isPlaceablePad, launchTitle, splitLaunches } from '../../lib/launches';
import { parseTime } from '../../lib/time';
import { useGlobe, useUi, useWorkspace } from '../../runtime';
import { type LayerCondition, layerCondition } from '../layers/status';
import { formatRelative, formatUtcMinute } from './format';
import { LaunchStatusChip } from './LaunchStatusChip';
import styles from './launches.module.css';
import { useLaunchClock, useLaunches } from './useLaunches';

const LOADING_ROWS = 3;

/**
 * Nothing to list because the feed failed: no snapshot yet, or one the server marked
 * unavailable with no launches in it (its answer to a failed first fetch). The body's error
 * state explains it; the status line neither repeats the reason nor counts launches it lacks.
 */
function isOutage(condition: LayerCondition, count: number, hasSnapshot: boolean): boolean {
  return count === 0 && (condition.kind === 'unavailable' || !hasSnapshot);
}

/** A reason as a sentence: upstream reasons often come without a full stop. */
function asSentence(text: string): string {
  const trimmed = text.trim();
  return /[.!?…]$/.test(trimmed) ? trimmed : `${trimmed}.`;
}

export function sourceNamesOf(layer: LayerDescriptor, catalog: Catalog): string {
  const names = new Map(catalog.sources.map((s) => [s.id, s.name]));
  return layer.sourceIds.map((id) => names.get(id) ?? id).join(', ');
}

interface LaunchRowProps {
  readonly launch: Launch;
  readonly nowMs: number;
  readonly selected: boolean;
  readonly onSelect: (launch: Launch) => void;
}

/**
 * One launch: its name with the relative NET on the same line, then status and UTC NET, who
 * launches it on what, and the pad. Each line ellipsises on its own, so a long pad name never
 * hides the time.
 */
const LaunchRow = memo(function LaunchRow({ launch, nowMs, selected, onSelect }: LaunchRowProps) {
  const net = parseTime(launch.net);
  const who = [launch.provider, launch.rocket].filter(Boolean).join(' · ');
  return (
    <ListRow
      data-launch-id={launch.id}
      title={
        <span className={styles.titleLine}>
          <span className={styles.titleText}>{launchTitle(launch)}</span>
          {net === null ? null : (
            <span className={styles.relative}>{formatRelative(net, nowMs)}</span>
          )}
        </span>
      }
      selected={selected}
      onSelect={() => onSelect(launch)}
      description={
        <span className={styles.rowLines}>
          <span className={styles.rowLine}>
            <LaunchStatusChip status={launch.status} />
            <span className="mono">{net === null ? 'NET not reported' : formatUtcMinute(net)}</span>
          </span>
          {awaitingOutcome(launch, nowMs) ? (
            <span className={styles.rowLine}>No outcome reported yet</span>
          ) : null}
          {who ? <span className={styles.rowLine}>{who}</span> : null}
          <span className={styles.rowLine}>{launch.pad?.name ?? 'Pad not reported'}</span>
        </span>
      }
    />
  );
});

interface LaunchGroupProps {
  readonly title: string;
  readonly empty: string;
  readonly launches: readonly Launch[];
  readonly nowMs: number;
  readonly selectedId: string | null;
  readonly onSelect: (launch: Launch) => void;
}

function LaunchGroup({ title, empty, launches, nowMs, selectedId, onSelect }: LaunchGroupProps) {
  const headingId = useId();
  return (
    <section className={styles.group} aria-labelledby={headingId}>
      <h3 id={headingId} className={styles.groupLabel}>
        {title}
        <span className={styles.groupCount}>{formatCount(launches.length)}</span>
      </h3>
      {launches.length === 0 ? (
        <p className={styles.groupEmpty}>{empty}</p>
      ) : (
        <List aria-label={`${title} launches`} selectable>
          {launches.map((launch) => (
            <LaunchRow
              key={launch.id}
              launch={launch}
              nowMs={nowMs}
              selected={launch.id === selectedId}
              onSelect={onSelect}
            />
          ))}
        </List>
      )}
    </section>
  );
}

/** Upcoming soonest first, then recent latest first, on the launch clock (to the minute). */
function LaunchGroups({ launches }: { readonly launches: readonly Launch[] }) {
  const minute = useLaunchClock();
  const { upcoming, recent } = useMemo(() => splitLaunches(launches, minute), [launches, minute]);
  const selectedId = useWorkspace((s) =>
    s.selection?.feed === 'launches' ? s.selection.id : null,
  );
  const select = useWorkspace((s) => s.select);
  const globe = useGlobe();
  // Selecting opens the inspector; the camera goes to the pad when there is one.
  const onSelect = useCallback(
    (launch: Launch) => {
      select({ feed: 'launches', id: launch.id });
      const pad = launch.pad;
      if (isPlaceablePad(pad)) {
        globe?.flyTo({ kind: 'point', position: { lat: pad.latitude, lon: pad.longitude } });
      }
    },
    [select, globe],
  );
  return (
    <>
      <LaunchGroup
        title="Upcoming"
        empty="No upcoming launches reported."
        launches={upcoming}
        nowMs={minute}
        selectedId={selectedId}
        onSelect={onSelect}
      />
      <LaunchGroup
        title="Recent"
        empty="No recent launches reported."
        launches={recent}
        nowMs={minute}
        selectedId={selectedId}
        onSelect={onSelect}
      />
    </>
  );
}

function LoadingRows() {
  return (
    <div className={styles.loading} aria-busy="true">
      <p className={styles.groupEmpty}>Connecting…</p>
      {Array.from({ length: LOADING_ROWS }, (_, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: static placeholders with no identity.
        <div key={i} className={styles.loadingRow}>
          <Skeleton width={`${70 - i * 12}%`} />
          <Skeleton width="45%" height="12px" />
        </div>
      ))}
    </div>
  );
}

/** The panel body for each state the layer can be in (design-system §11). */
function LaunchesBody({ layer }: { readonly layer: LayerDescriptor }) {
  const state = useWorkspace((s) => s.layers.launches);
  const setLayerEnabled = useWorkspace((s) => s.setLayerEnabled);
  const setSidePanel = useUi((s) => s.setSidePanel);
  const { launches, hasSnapshot, summary } = useLaunches();
  const condition = layerCondition(layer, state, summary);

  if (condition.kind === 'unconfigured' || condition.kind === 'licence') {
    return (
      <EmptyState
        icon={RocketLaunchIcon}
        action={
          <Button size="sm" variant="secondary" onClick={() => setSidePanel('sources')}>
            Open Sources
          </Button>
        }
      >
        The {layer.name} layer cannot be turned on here: {condition.reason}.
      </EmptyState>
    );
  }
  if (condition.kind === 'off') {
    return (
      <EmptyState
        icon={RocketLaunchIcon}
        action={
          <Button size="sm" onClick={() => setLayerEnabled('launches', true)}>
            Turn on {layer.name}
          </Button>
        }
      >
        The {layer.name} layer is off. Turn it on to list recent and upcoming launches and mark
        their pads on the globe.
      </EmptyState>
    );
  }
  if (condition.kind === 'connecting') return <LoadingRows />;

  if (isOutage(condition, launches.length, hasSnapshot)) {
    const reason = condition.reason ?? summary?.error?.message ?? 'No launch data has arrived';
    return (
      <ErrorState live title={`The ${layer.name} layer is unavailable`}>
        {asSentence(reason)} Lavender keeps trying.
      </ErrorState>
    );
  }
  if (launches.length === 0) {
    return (
      <EmptyState icon={RocketLaunchIcon}>
        The source reports no recent or upcoming launches.
      </EmptyState>
    );
  }
  return <LaunchGroups launches={launches} />;
}

/**
 * The line under the header: trouble as a status pill with its reason, the count, and the
 * feed's notices (fixture replays say so here). Live data shows no pill, as in the layer row.
 */
function LaunchesStatus({ layer }: { readonly layer: LayerDescriptor }) {
  const state = useWorkspace((s) => s.layers.launches);
  const { launches, hasSnapshot, summary } = useLaunches();
  if (!state?.enabled || !summary) return null;
  const condition = layerCondition(layer, state, summary);
  // The count and the reason need data to qualify; without any, the body's error state says why.
  const ready =
    !isOutage(condition, launches.length, hasSnapshot) &&
    (summary.phase === 'ready' || summary.fetchedAt !== null);
  return (
    <div className={styles.toolbar}>
      {condition.pill || ready ? (
        <p className={styles.statusLine}>
          {condition.pill ? (
            <StatusPill
              size="sm"
              status={condition.pill}
              {...(condition.pillDetail ? { detail: condition.pillDetail } : {})}
            />
          ) : null}
          {ready ? (
            <span>
              {formatCount(launches.length)} {launches.length === 1 ? 'launch' : 'launches'}
            </span>
          ) : null}
          {ready && condition.reason ? (
            <span className={styles.reason}>{condition.reason}</span>
          ) : null}
        </p>
      ) : null}
      {summary.notices.map((notice) => (
        <p key={notice} className={styles.notice}>
          <InfoIcon size={14} weight="fill" aria-hidden className={styles.noticeIcon} />
          {notice}
        </p>
      ))}
    </div>
  );
}

export interface LaunchesPanelProps {
  readonly catalog: Catalog;
  readonly className?: string | undefined;
}

/**
 * The launch list (left island): upcoming launches soonest first with their relative and UTC
 * times, recent ones below, each with its status, provider, rocket and pad. Selecting one
 * selects it, flies to its pad and opens the inspector. Every pad marker on the globe has its
 * launches here (design-system §13).
 */
export function LaunchesPanel({ catalog, className }: LaunchesPanelProps) {
  const layer = catalog.layers.find((l) => l.feed === 'launches');
  const setSidePanel = useUi((s) => s.setSidePanel);
  const title = layer?.name ?? 'Rocket launches';
  const sources = layer ? sourceNamesOf(layer, catalog) : '';
  return (
    <Panel
      title={title}
      dense
      className={className}
      actions={
        <>
          <IconButton label="Show layers" icon={StackIcon} onClick={() => setSidePanel('layers')} />
          <IconButton
            label={`Close ${title.toLowerCase()}`}
            icon={XIcon}
            onClick={() => setSidePanel(null)}
          />
        </>
      }
      toolbar={layer ? <LaunchesStatus layer={layer} /> : null}
      footer={sources ? <span className={styles.footnote}>Source: {sources}</span> : null}
    >
      {layer ? (
        <LaunchesBody layer={layer} />
      ) : (
        <EmptyState icon={RocketLaunchIcon}>This server does not offer rocket launches.</EmptyState>
      )}
    </Panel>
  );
}
