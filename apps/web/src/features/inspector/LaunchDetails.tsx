/**
 * The launch-specific parts of the inspector: the mission summary, payloads, the satellites
 * matched to them (as links that select the satellite when the satellites layer has it), the
 * other launches from the same pad, the pad sheet's launch list, the source's attribution and
 * the feed's notices (a fixture replay says so on every launch sheet).
 */
import { useMemo, useState } from 'react';
import type { Launch } from '../../contracts';
import { Button, InfoIcon } from '../../design';
import type { EntityRef } from '../../globe';
import {
  groupLaunchPads,
  isPadId,
  type LaunchPadGroup,
  launchTitle,
  splitLaunches,
} from '../../lib/launches';
import { parseTime } from '../../lib/time';
import { useSystem, useWorkspace } from '../../runtime';
import {
  formatRelative,
  LaunchStatusChip,
  useLatestSnapshot,
  useLaunchClock,
  useLaunches,
} from '../launches';
import { layerCondition } from '../layers/status';
import type { Inspected } from './facts';
import inspector from './inspector.module.css';
import styles from './launchDetails.module.css';

/** The latest launches grouped by pad, recomputed when a new snapshot arrives. */
function useLaunchPads(): readonly LaunchPadGroup[] {
  const { launches } = useLaunches();
  return useMemo(() => groupLaunchPads(launches), [launches]);
}

export interface PadView {
  readonly view: Inspected;
  /** Whether the pad is in the latest snapshot (else its last known launches are shown). */
  readonly present: boolean;
}

/**
 * A selected launch pad (`pad:<lat>,<lon>`) read from the latest launches snapshot, since the
 * feed itself lists launches, not pads. Null for any other selection, and for a pad that has
 * never been seen; a pad that left the feed keeps its last known launches.
 */
export function useLaunchPadView(selection: EntityRef): PadView | null {
  const isPad = selection.feed === 'launches' && isPadId(selection.id);
  const { launches, hasSnapshot } = useLaunches();
  // Only a pad selection groups the launches; any other sheet skips the work.
  const found = useMemo(
    () =>
      isPad && hasSnapshot
        ? groupLaunchPads(launches).find((pad) => pad.id === selection.id)
        : undefined,
    [isPad, hasSnapshot, launches, selection.id],
  );
  const [held, setHeld] = useState<LaunchPadGroup | null>(null);
  if (found && found !== held) setHeld(found);
  const pad = found ?? held;
  if (!isPad || !pad) return null;
  return { view: { kind: 'launchPad', item: pad }, present: found !== undefined };
}

/** The feed's notices, at the top of a launch or pad sheet ("Recorded fixture data, not live."). */
export function LaunchNotices() {
  const { summary } = useLatestSnapshot('launches');
  const notices = summary?.notices ?? [];
  if (notices.length === 0) return null;
  return (
    <div className={styles.notices}>
      {notices.map((notice) => (
        <p key={notice} className={styles.notice}>
          <InfoIcon size={14} weight="fill" aria-hidden className={styles.noticeIcon} />
          {notice}
        </p>
      ))}
    </div>
  );
}

/**
 * The provider's attribution, verbatim from the catalog as the Sources panel shows it (the
 * source's name is in the provenance below; it prefixes the text only when there are several).
 */
function Attribution() {
  const catalog = useSystem((s) => s.catalog);
  const layer = catalog?.layers.find((l) => l.feed === 'launches');
  const attributions = (layer?.sourceIds ?? []).flatMap((id) => {
    const source = catalog?.sources.find((s) => s.id === id);
    return source ? [{ id, name: source.name, attribution: source.attribution }] : [];
  });
  if (attributions.length === 0) return null;
  return (
    <>
      <h3 className={inspector.subhead}>Attribution</h3>
      {attributions.map((a) => (
        <p key={a.id} className={styles.prose} data-attribution="">
          {attributions.length > 1 ? `${a.name}: ${a.attribution}` : a.attribution}
        </p>
      ))}
    </>
  );
}

interface LaunchLinksProps {
  readonly label: string;
  readonly launches: readonly Launch[];
  readonly nowMs: number;
}

/** Launches as links that select them: name, status and when. */
function LaunchLinks({ label, launches, nowMs }: LaunchLinksProps) {
  const select = useWorkspace((s) => s.select);
  return (
    <ul className={styles.links} aria-label={label}>
      {launches.map((launch) => {
        const net = parseTime(launch.net);
        return (
          <li key={launch.id} className={styles.linkRow}>
            <button
              type="button"
              className={styles.entityLink}
              onClick={() => select({ feed: 'launches', id: launch.id })}
            >
              {launchTitle(launch)}
            </button>
            <span className={styles.linkMeta}>
              <LaunchStatusChip status={launch.status} />
              <span className="tabular">
                {net === null ? 'NET not reported' : formatRelative(net, nowMs)}
              </span>
            </span>
          </li>
        );
      })}
    </ul>
  );
}

function Payloads({ names }: { readonly names: readonly string[] }) {
  return (
    <>
      <h3 className={inspector.subhead}>Payloads</h3>
      {names.length === 0 ? (
        <p className={styles.prose}>No payloads listed by the source.</p>
      ) : (
        <ul className={styles.plain} aria-label="Payloads">
          {names.map((name) => (
            <li key={name}>{name}</li>
          ))}
        </ul>
      )}
    </>
  );
}

/**
 * Satellites matched to this launch on the server, as catalogue numbers. Each is a link that
 * selects the satellite when the satellites layer currently has it; otherwise the sheet says
 * why it cannot be selected and, when it helps, offers to turn the layer on.
 */
export function MatchedSatellites({ noradIds }: { readonly noradIds: readonly number[] }) {
  const catalog = useSystem((s) => s.catalog);
  const layer = catalog?.layers.find((l) => l.feed === 'satellites');
  const state = useWorkspace((s) => s.layers.satellites);
  const setLayerEnabled = useWorkspace((s) => s.setLayerEnabled);
  const select = useWorkspace((s) => s.select);
  const { snapshot, summary } = useLatestSnapshot('satellites');
  const names = useMemo(
    () =>
      new Map(
        snapshot?.items.kind === 'satellites'
          ? snapshot.items.items.map((s) => [s.noradId, s.name] as const)
          : [],
      ),
    [snapshot],
  );

  if (noradIds.length === 0) {
    return (
      <>
        <h3 className={inspector.subhead}>Matched satellites</h3>
        <p className={styles.prose}>
          No tracked satellites are matched to this launch. Matches appear once a launch has reached
          orbit and Launch Library 2 has given it a launch designator.
        </p>
      </>
    );
  }

  const layerName = layer?.name ?? 'Satellites';
  const shown = snapshot !== undefined;
  const missing = shown ? noradIds.filter((id) => !names.has(id)) : [];
  let note: string | null = null;
  let turnOn = false;
  if (!layer) {
    note = 'This server offers no satellites layer, so they cannot be selected here.';
  } else if (layer.availability.state !== 'available') {
    note = `The ${layerName} layer is unavailable here, so they cannot be selected.`;
  } else if (!state?.enabled) {
    note = `Turn on ${layerName} to select them.`;
    turnOn = true;
  } else if (!shown) {
    const condition = layerCondition(layer, state, summary);
    note =
      condition.kind === 'unavailable'
        ? `${layerName} data is unavailable${condition.reason ? `: ${condition.reason}` : ''}.`
        : `Waiting for ${layerName} data…`;
  } else if (missing.length > 0) {
    note = `${missing.length === noradIds.length ? 'They are' : 'Some are'} not in the ${layerName} groups shown; the Recent launches group lists newly launched satellites.`;
  }

  return (
    <>
      <h3 className={inspector.subhead}>Matched satellites</h3>
      <ul className={styles.links} aria-label="Matched satellites">
        {noradIds.map((noradId) => {
          const name = names.get(noradId);
          return (
            <li key={noradId} className={styles.linkRow}>
              {name ? (
                <button
                  type="button"
                  className={styles.entityLink}
                  onClick={() => select({ feed: 'satellites', id: String(noradId) })}
                >
                  {name}
                  <span className="mono"> · NORAD {noradId}</span>
                </button>
              ) : (
                <span className="mono">NORAD {noradId}</span>
              )}
              {!name && shown ? (
                <span className={styles.linkMeta}>Not in the groups shown</span>
              ) : null}
            </li>
          );
        })}
      </ul>
      {note ? <p className={styles.prose}>{note}</p> : null}
      {turnOn ? (
        <Button size="sm" variant="secondary" onClick={() => setLayerEnabled('satellites', true)}>
          Turn on {layerName}
        </Button>
      ) : null}
    </>
  );
}

/** The other launches from this launch's pad, upcoming first. */
function PadNeighbours({ launch }: { readonly launch: Launch }) {
  const nowMs = useLaunchClock();
  const pads = useLaunchPads();
  const pad = pads.find((group) => group.launches.some((l) => l.id === launch.id));
  if (!pad || pad.launches.length < 2) return null;
  const { upcoming, recent } = splitLaunches(
    pad.launches.filter((l) => l.id !== launch.id),
    nowMs,
  );
  return (
    <>
      <h3 className={inspector.subhead}>Also from {pad.name}</h3>
      <LaunchLinks
        label={`Other launches from ${pad.name}`}
        launches={[...upcoming, ...recent]}
        nowMs={nowMs}
      />
    </>
  );
}

/** A launch sheet's sections after its facts. */
export function LaunchDetails({ launch }: { readonly launch: Launch }) {
  const description = launch.missionDescription?.trim();
  return (
    <>
      {description ? (
        <>
          <h3 className={inspector.subhead}>Mission</h3>
          <p className={styles.prose}>{description}</p>
        </>
      ) : null}
      <Payloads names={launch.payloadNames} />
      <MatchedSatellites noradIds={launch.matchedNoradIds} />
      <PadNeighbours launch={launch} />
      <Attribution />
    </>
  );
}

/** A pad sheet's sections after its facts: its launches, upcoming then recent. */
export function PadLaunches({ pad }: { readonly pad: LaunchPadGroup }) {
  const nowMs = useLaunchClock();
  const { upcoming, recent } = splitLaunches(pad.launches, nowMs);
  return (
    <>
      <h3 className={inspector.subhead}>Upcoming launches</h3>
      {upcoming.length === 0 ? (
        <p className={styles.prose}>None reported.</p>
      ) : (
        <LaunchLinks label="Upcoming launches from this pad" launches={upcoming} nowMs={nowMs} />
      )}
      <h3 className={inspector.subhead}>Recent launches</h3>
      {recent.length === 0 ? (
        <p className={styles.prose}>None reported.</p>
      ) : (
        <LaunchLinks label="Recent launches from this pad" launches={recent} nowMs={nowMs} />
      )}
      <Attribution />
    </>
  );
}
