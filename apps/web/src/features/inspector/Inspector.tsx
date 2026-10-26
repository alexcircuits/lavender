import {
  type ReactNode,
  type RefObject,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import type { Catalog, FeedId, GeoPoint, LayerDescriptor } from '../../contracts';
import {
  ArrowSquareOutIcon,
  Button,
  CopyIcon,
  CrosshairIcon,
  EmptyState,
  formatCount,
  IconButton,
  InfoIcon,
  NavigationArrowIcon,
  Panel,
  SegmentedControl,
  showToast,
  WarningIcon,
  XIcon,
} from '../../design';
import type { EntityRef, TrackingState } from '../../globe';
import { formatDecimal } from '../../lib/coordinates';
import { isPadId, isPlaceablePad, launchClock, launchTitle } from '../../lib/launches';
import { noaaRecordUrl, usgsEventUrl } from '../../lib/safeUrl';
import { formatUtcTime, parseTime } from '../../lib/time';
import { useNow } from '../../lib/useNow';
import { useFeeds, useGlobe, useSystem, useWorkspace } from '../../runtime';
import type { FeedSummary } from '../../state/stores';
import { GLOBE_REGION_ID } from '../commands';
import { FamilySwatch, feedGlyph } from '../layers';
import { AircraftHistory } from './AircraftHistory';
import { AirQualityCard } from './AirQualityCard';
import { bikeshareFacts } from './BikeshareCard';
import { BoundaryCard } from './BoundaryCard';
import { CableCard } from './CableCard';
import { CctvCard } from './CctvCard';
import { CycloneForecast } from './CycloneForecast';
import { EntityResolutionCard } from './EntityResolutionCard';
import { FactSheet } from './FactSheet';
import {
  aircraftFacts,
  cycloneFacts,
  earthquakeFacts,
  type Fact,
  fireFacts,
  type Inspected,
  inspect,
  positionFacts,
  provenanceFacts,
  satelliteFacts,
  satelliteGroupLabel,
  vesselFacts,
} from './facts';
import styles from './inspector.module.css';
import { LaunchDetails, LaunchNotices, PadLaunches, useLaunchPadView } from './LaunchDetails';
import { launchFacts, padFacts } from './launchFacts';
import { SatellitePasses } from './SatellitePasses';
import { SearchAroundCard } from './SearchAroundCard';
import { transitFacts } from './TransitCard';
import { useSatellitePosition } from './useSatellitePosition';
import { VesselTrack, vesselReportedAtSec } from './VesselTrack';

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

export interface HeroMetric {
  readonly label: string;
  readonly value: string;
  readonly sub?: string | undefined;
}

export function extractHeroMetrics(view: Inspected): HeroMetric[] {
  switch (view.kind) {
    case 'aircraft': {
      const a = view.item;
      const alt = a.onGround
        ? 'Ground'
        : a.altitudeBaroFt !== undefined
          ? `${Math.round(a.altitudeBaroFt).toLocaleString()} ft`
          : a.altitudeGeomFt !== undefined
            ? `${Math.round(a.altitudeGeomFt).toLocaleString()} ft`
            : '—';
      const spd = a.groundSpeedKt !== undefined ? `${Math.round(a.groundSpeedKt)} kt` : '—';
      const hdg = a.trackDeg !== undefined ? `${Math.round(a.trackDeg)}°` : '—';
      return [
        {
          label: 'Altitude',
          value: alt,
          sub: a.onGround ? undefined : a.altitudeBaroFt ? 'baro' : 'geom',
        },
        { label: 'Speed', value: spd, sub: 'ground' },
        { label: 'Track', value: hdg, sub: 'heading' },
      ];
    }
    case 'vessels': {
      const v = view.item;
      const spd = v.speedKnots !== undefined ? `${v.speedKnots.toFixed(1)} kn` : '—';
      const course = v.courseDeg !== undefined ? `${Math.round(v.courseDeg)}°` : '—';
      const heading =
        v.headingDeg !== undefined
          ? `${Math.round(v.headingDeg)}°`
          : v.vesselType
            ? v.vesselType.slice(0, 12)
            : '—';
      return [
        { label: 'Speed', value: spd, sub: 'SOG' },
        { label: 'Course', value: course, sub: 'COG' },
        { label: 'Heading', value: heading, sub: v.headingDeg !== undefined ? 'true' : 'type' },
      ];
    }
    case 'satellites': {
      const s = view.item;
      return [
        { label: 'NORAD', value: String(s.noradId), sub: s.group },
        { label: 'Inclination', value: `${s.inclinationDeg.toFixed(1)}°`, sub: 'orbit' },
        { label: 'Motion', value: `${s.meanMotion.toFixed(1)} rev/d`, sub: 'mean' },
      ];
    }
    case 'cyclones': {
      const c = view.item;
      const wind = c.intensityKt !== undefined ? `${c.intensityKt} kt` : '—';
      const pres = c.pressureMb !== undefined ? `${c.pressureMb} hPa` : '—';
      const cat = c.classification ? c.classification.slice(0, 12) : 'Active';
      return [
        { label: 'Intensity', value: wind, sub: 'max winds' },
        { label: 'Pressure', value: pres, sub: 'central' },
        { label: 'Class', value: cat, sub: c.basin },
      ];
    }
    case 'earthquakes': {
      const eq = view.item;
      return [
        {
          label: 'Magnitude',
          value: `M ${eq.magnitude.toFixed(1)}`,
          sub: eq.magnitudeType ?? undefined,
        },
        { label: 'Depth', value: `${eq.depthKm.toFixed(1)} km` },
        { label: 'Status', value: eq.status ?? 'Reviewed' },
      ];
    }
    case 'fires': {
      const f = view.item;
      return [
        { label: 'FRP', value: `${f.frp.toFixed(1)} MW`, sub: 'power' },
        { label: 'Brightness', value: `${Math.round(f.brightness)} K` },
        { label: 'Confidence', value: f.confidence },
      ];
    }
    case 'launches': {
      const l = view.item;
      return [
        { label: 'Status', value: l.status.slice(0, 16) },
        { label: 'Provider', value: l.provider?.slice(0, 12) ?? 'Space' },
        { label: 'Designator', value: l.launchDesignator ?? '—' },
      ];
    }
    default:
      return [];
  }
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
    case 'vessels': {
      const vessel = view.item;
      return {
        title: vessel.name?.trim() || vessel.mmsi,
        summary: `MMSI ${vessel.mmsi}`,
        facts: vesselFacts(vessel, nowMs),
        position: vessel.position,
        observedMs: parseTime(vessel.lastPositionUtc),
        sourceIds: snapshotSources,
        recordUrl: null,
      };
    }
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
    case 'fires': {
      const cell = view.item;
      const isCluster = cell.count > 1;
      return {
        title: isCluster
          ? `Fire cluster (${formatCount(cell.count)} detections)`
          : 'Active fire detection',
        summary: `FRP ${cell.frp.toFixed(1)} MW · ${cell.satellite || 'Satellite'}`,
        facts: fireFacts(cell),
        position: { lat: cell.lat, lon: cell.lon },
        observedMs: parseTime(cell.latestAcquiredAt || cell.acquiredAt),
        sourceIds: snapshotSources,
        recordUrl: null,
      };
    }
    case 'launches': {
      const launch = view.item;
      const pad = isPlaceablePad(launch.pad) ? launch.pad : null;
      return {
        title: launchTitle(launch),
        summary: [launch.provider, launch.rocket].filter(Boolean).join(' · ') || 'Rocket launch',
        // The launch clock, as the list beside this sheet reads it.
        facts: launchFacts(launch, launchClock(nowMs)),
        position: pad ? { lat: pad.latitude, lon: pad.longitude } : null,
        observedMs: null,
        sourceIds: snapshotSources,
        recordUrl: null,
      };
    }
    case 'launchPad': {
      const pad = view.item;
      return {
        title: pad.name,
        summary: pad.locationName ? `Launch pad · ${pad.locationName}` : 'Launch pad',
        facts: padFacts(pad, launchClock(nowMs)),
        position: { lat: pad.latitude, lon: pad.longitude },
        observedMs: null,
        sourceIds: snapshotSources,
        recordUrl: null,
      };
    }
    case 'bikeshare': {
      const station = view.item;
      const bikes = station.bikesAvailable ?? 0;
      const docks = station.docksAvailable ?? 0;
      return {
        title: station.name,
        summary: `${station.systemId} · ${bikes} bikes · ${docks} docks`,
        facts: bikeshareFacts(station),
        position: station.position,
        observedMs: fetchedMs,
        sourceIds: snapshotSources,
        recordUrl: null,
      };
    }
    case 'transit': {
      const v = view.item;
      return {
        title: v.routeId ? `Route ${v.routeId} • Vehicle ${v.vehicleId}` : `Vehicle ${v.vehicleId}`,
        summary: `${v.feedId}${v.speedMps !== undefined ? ` · ${(v.speedMps * 3.6).toFixed(0)} km/h` : ''}`,
        facts: transitFacts(v),
        position: v.position,
        observedMs: v.timestamp ? Number(v.timestamp) * 1000 : fetchedMs,
        sourceIds: snapshotSources,
        recordUrl: null,
      };
    }
  }
}

const isPadSelection = (selection: EntityRef) =>
  selection.feed === 'launches' && isPadId(selection.id);

/** Why the selected entity has no record to show, and what will change that. */
function emptyMessage(
  selection: EntityRef,
  name: string,
  canTurnOn: boolean,
  waiting: boolean,
): string {
  const subject = isPadSelection(selection) ? 'This launch pad' : selection.id;
  if (canTurnOn) return `${subject} is in ${name}, which is off. Turn it on to see it.`;
  if (waiting) return `Waiting for ${name} data…`;
  // Aircraft, earthquakes and fires are fetched for the area the globe shows; satellites worldwide.
  if (
    selection.feed === 'aircraft' ||
    selection.feed === 'earthquakes' ||
    selection.feed === 'vessels' ||
    selection.feed === 'fires'
  ) {
    return `${subject} is not in the latest ${name} data for the area the globe shows.`;
  }
  return `${subject} is not in the latest ${name} data.`;
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

function BoundaryInspector({ selection, className }: InspectorBodyProps) {
  const globe = useGlobe();
  const boundary = globe?.getBoundaryFeature(selection.id);
  if (boundary) {
    return <BoundaryCard feature={boundary} className={className} />;
  }
  return (
    <InspectorFrame title={selection.id} layer={undefined} className={className}>
      <EmptyState icon={InfoIcon}>Boundary {selection.id} is not loaded.</EmptyState>
    </InspectorFrame>
  );
}

function CableInspector({ selection, className }: InspectorBodyProps) {
  const globe = useGlobe();
  const cable = globe?.getCable(selection.id);
  if (cable) {
    return <CableCard cable={cable} className={className} />;
  }
  return (
    <InspectorFrame title={selection.id} layer={undefined} className={className}>
      <EmptyState icon={InfoIcon}>Submarine cable {selection.id} is not loaded.</EmptyState>
    </InspectorFrame>
  );
}

function CctvInspector({ selection, className }: InspectorBodyProps) {
  const globe = useGlobe();
  const camera = globe?.getCctvCamera(selection.id);
  if (camera) {
    return <CctvCard camera={camera} className={className} />;
  }
  return (
    <InspectorFrame title={selection.id} layer={undefined} className={className}>
      <EmptyState icon={InfoIcon}>Camera {selection.id} is not loaded.</EmptyState>
    </InspectorFrame>
  );
}

function AirQualityInspector({ selection, className }: InspectorBodyProps) {
  const globe = useGlobe();
  const reading = globe?.getAirQualityReading(selection.id);
  if (reading) {
    return <AirQualityCard reading={reading} className={className} />;
  }
  return (
    <InspectorFrame title={selection.id} layer={undefined} className={className}>
      <EmptyState icon={InfoIcon}>Air quality observation {selection.id} is not loaded.</EmptyState>
    </InspectorFrame>
  );
}

function InspectorBody({ selection, className }: InspectorBodyProps) {
  const globe = useGlobe();
  const feed = selection.feed as FeedId;

  const entity = useFeeds((s) => s.selectedEntity);
  const storePresent = useFeeds((s) => s.selectedPresent);
  const lastSeen = useFeeds((s) => s.selectedLastSeen);
  const summary = useFeeds((s) => s.summaries[feed]);
  const catalog = useSystem((s) => s.catalog);
  const enabled = useWorkspace((s) => s.layers[feed]?.enabled ?? false);
  const setLayerEnabled = useWorkspace((s) => s.setLayerEnabled);
  const nowMs = useNow();
  const layer = catalog?.layers.find((l) => l.feed === feed);
  const storeView = useMemo(
    () => inspect(feed, selection.id, entity),
    [feed, selection.id, entity],
  );
  // A launch pad is not a feed item: its sheet is read from the latest launches snapshot.
  const padView = useLaunchPadView({ feed, id: selection.id });
  const view = padView?.view ?? storeView;
  const present = padView ? padView.present : storePresent;
  const noradId = feed === 'satellites' ? Number(selection.id) : Number.NaN;
  const satellite = useSatellitePosition(Number.isInteger(noradId) && noradId > 0 ? noradId : null);

  const isTrackable = feed === 'aircraft' || feed === 'vessels' || feed === 'satellites';

  const [followState, setFollowState] = useState<TrackingState | null>(
    () => globe?.getFollowState() ?? null,
  );

  useEffect(() => {
    if (!globe) return;
    setFollowState(globe.getFollowState());
    return globe.onFollowChange((state) => {
      setFollowState(state.status === 'released' ? null : state);
    });
  }, [globe]);

  const isFollowingThis =
    followState !== null &&
    followState.status !== 'released' &&
    followState.target?.feed === feed &&
    followState.target?.id === selection.id;

  const [activeTab, setActiveTab] = useState<
    'all' | 'telemetry' | 'identity' | 'history' | 'provenance'
  >('all');
  const metrics = useMemo(() => (view ? extractHeroMetrics(view) : []), [view]);

  if (!view) {
    const name = layer?.name ?? feed;
    const canTurnOn = !enabled && layer?.availability.state === 'available';
    const waiting = enabled && (!summary || summary.phase === 'connecting');
    return (
      <InspectorFrame
        title={isPadSelection({ feed, id: selection.id }) ? 'Launch pad' : selection.id}
        layer={layer}
        className={className}
      >
        <EmptyState
          icon={InfoIcon}
          action={
            canTurnOn ? (
              <Button size="sm" onClick={() => setLayerEnabled(feed, true)}>
                Turn on {name}
              </Button>
            ) : null
          }
        >
          {emptyMessage({ feed, id: selection.id }, name, canTurnOn, waiting)}
        </EmptyState>
      </InspectorFrame>
    );
  }

  const sheet = sheetFor(view, summary, satellite, nowMs);
  const lastSeenMs = parseTime(lastSeen);
  // Launch and pad sheets show the feed's notices prominently at the top; say them once.
  const noticesOnTop = view.kind === 'launches' || view.kind === 'launchPad';
  const provenance = provenanceFacts({
    sources: sourceNames(
      sheet.sourceIds.length > 0 ? sheet.sourceIds : (layer?.sourceIds ?? []),
      catalog,
    ),
    observedMs: sheet.observedMs,
    fetchedMs: parseTime(summary?.fetchedAt),
    nowMs,
    notices: noticesOnTop ? [] : (summary?.notices ?? []),
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

      {metrics.length > 0 ? (
        <div className={styles.metricGrid}>
          {metrics.map((m) => (
            <div key={m.label} className={styles.metricCard}>
              <span className={styles.metricLabel}>{m.label}</span>
              <span className={styles.metricValue}>{m.value}</span>
              {m.sub ? <span className={styles.metricSub}>{m.sub}</span> : null}
            </div>
          ))}
        </div>
      ) : null}

      <div className={styles.quickActions}>
        {isTrackable ? (
          isFollowingThis ? (
            <Button
              size="sm"
              variant="secondary"
              icon={CrosshairIcon}
              onClick={() => globe?.stopFollowing()}
            >
              Stop following
            </Button>
          ) : (
            <Button
              size="sm"
              variant="secondary"
              icon={CrosshairIcon}
              disabled={!globe || !position}
              onClick={() => globe?.startFollowing(selection)}
            >
              Follow
            </Button>
          )
        ) : null}
        <Button
          size="sm"
          variant="primary"
          icon={NavigationArrowIcon}
          disabled={!globe || !position}
          onClick={() => position && globe?.flyTo({ kind: 'point', position })}
        >
          Fly to
        </Button>
        <Button
          size="sm"
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

      <SegmentedControl<'all' | 'telemetry' | 'identity' | 'history' | 'provenance'>
        aria-label="Inspector views"
        size="sm"
        value={activeTab}
        onValueChange={setActiveTab}
        options={[
          { value: 'all', label: 'All' },
          { value: 'telemetry', label: 'Telemetry' },
          { value: 'identity', label: 'Identity' },
          { value: 'history', label: 'History' },
          { value: 'provenance', label: 'Provenance' },
        ]}
        className={styles.viewTabs}
      />

      {noticesOnTop ? <LaunchNotices /> : null}

      <div hidden={activeTab !== 'all' && activeTab !== 'telemetry'}>
        <div className={styles.sectionCard}>
          <div className={styles.cardHeader}>
            <h4 className={styles.cardTitle}>Telemetry & Specifications</h4>
          </div>
          <FactSheet facts={sheet.facts} label="Facts" />
          {view.kind === 'launches' ? <LaunchDetails launch={view.item} /> : null}
          {view.kind === 'launchPad' ? <PadLaunches pad={view.item} /> : null}
        </div>
      </div>

      <div hidden={activeTab !== 'all' && activeTab !== 'identity'}>
        {view.kind === 'aircraft' ? (
          <EntityResolutionCard feed="aircraft" entityId={view.item.hex} />
        ) : null}
        {view.kind === 'vessels' ? (
          <EntityResolutionCard feed="vessels" entityId={view.item.mmsi} />
        ) : null}
        {view.kind === 'satellites' ? (
          <EntityResolutionCard feed="satellites" entityId={String(view.item.noradId)} />
        ) : null}
        {position ? (
          <SearchAroundCard feed={feed} entityId={selection.id} position={position} />
        ) : null}
      </div>

      <div hidden={activeTab !== 'all' && activeTab !== 'history'}>
        {view.kind === 'aircraft' ? (
          <AircraftHistory key={view.item.hex} hex={view.item.hex} />
        ) : null}
        {view.kind === 'vessels' ? (
          <VesselTrack
            key={view.item.mmsi}
            mmsi={view.item.mmsi}
            reportedAtSec={vesselReportedAtSec(view.item)}
          />
        ) : null}
        {view.kind === 'satellites' ? (
          <SatellitePasses key={view.item.noradId} noradId={view.item.noradId} />
        ) : null}
        {view.kind === 'cyclones' ? <CycloneForecast storm={view.item} /> : null}
      </div>

      <div hidden={activeTab !== 'all' && activeTab !== 'provenance'}>
        <div className={styles.sectionCard}>
          <div className={styles.cardHeader}>
            <h4 className={styles.cardTitle}>Data Provenance & Audit</h4>
          </div>
          <FactSheet facts={provenance} label="Provenance" />
        </div>
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
  if (selection.feed === 'boundaries') {
    return (
      <BoundaryInspector
        key={`${selection.feed}:${selection.id}`}
        selection={selection}
        className={className}
      />
    );
  }
  if (selection.feed === 'cables') {
    return (
      <CableInspector
        key={`${selection.feed}:${selection.id}`}
        selection={selection}
        className={className}
      />
    );
  }
  if (selection.feed === 'cctv') {
    return (
      <CctvInspector
        key={`${selection.feed}:${selection.id}`}
        selection={selection}
        className={className}
      />
    );
  }
  if (selection.feed === 'air-quality') {
    return (
      <AirQualityInspector
        key={`${selection.feed}:${selection.id}`}
        selection={selection}
        className={className}
      />
    );
  }
  // A new selection is a new sheet: per-entity state (satellite position) starts fresh.
  return (
    <InspectorBody
      key={`${selection.feed}:${selection.id}`}
      selection={selection}
      className={className}
    />
  );
}
