import { lazy, Suspense, useEffect, useMemo, useRef, useState } from 'react';
import { Button, IconButton, MagnifyingGlassIcon } from '../design';

// Code-split with the other Wave B panels to hold the startup budget.
const BriefingModal = lazy(() =>
  import('../features/briefing').then((m) => ({ default: m.BriefingModal })),
);
const HudOverlay = lazy(() => import('../features/hud').then((m) => ({ default: m.HudOverlay })));
const ScopeMask = lazy(() => import('../features/scope').then((m) => ({ default: m.ScopeMask })));
const CyberSonarOverlay = lazy(() =>
  import('../features/sonar').then((m) => ({ default: m.CyberSonarOverlay })),
);
const CockpitPfd = lazy(() =>
  import('../features/cockpit').then((m) => ({ default: m.CockpitPfd })),
);

import type { Aircraft } from '../contracts';
import {
  GLOBE_REGION_ID,
  ShortcutSheet,
  useCommandContext,
  useGlobalShortcuts,
} from '../features/commands';
import { ImagerySplitDivider } from '../features/imagery/ImagerySplitDivider';
import { Inspector } from '../features/inspector';
import { LayerAnnouncer, useBoundariesLayer } from '../features/layers';
import { CommandPalette, SearchBar } from '../features/search';
import type { MountGlobe, TrackingState } from '../globe';
import { useFeeds, useGlobe, usePreferences, useUi, useWorkspace } from '../runtime';
import { CreditsButton } from './CreditsButton';
import { FeedStatusChip } from './FeedStatusChip';
import { FollowingChip } from './FollowingChip';
import { IslandBoundary } from './IslandBoundary';
import { useCollapsed, useRailFocusReturn } from './islandLayout';
import { MapControls } from './MapControls';
import { NavRail } from './NavRail';
import { SidePanel } from './SidePanel';
import { StatusBar } from './StatusBar';
import styles from './shell.module.css';
import { useMountedGlobe } from './useMountedGlobe';

export interface AppShellProps {
  readonly mountGlobe: MountGlobe;
}

/**
 * The application shell (design-system §6): the globe as the ground, with the navigation rail,
 * the left island, search, feed health, the inspector, map controls and the status bar sitting
 * on it as opaque islands.
 */
export function AppShell({ mountGlobe }: AppShellProps) {
  const { hostRef, creditsRef, status } = useMountedGlobe(mountGlobe);
  useBoundariesLayer();
  const commandContext = useCommandContext();
  useGlobalShortcuts(commandContext);
  const collapsed = useCollapsed();
  const setPaletteOpen = useUi((s) => s.setPaletteOpen);
  const setBriefingOpen = useUi((s) => s.setBriefingOpen);
  const sidePanel = useUi((s) => s.sidePanel);
  const visualPreset = usePreferences((s) => s.visualPreset);
  const hudEnabled = usePreferences((s) => s.hudEnabled);
  const scopeMaskEnabled = usePreferences((s) => s.scopeMaskEnabled);
  const sonarEnabled = usePreferences((s) => s.sonarEnabled);
  const selectionKey = useWorkspace((s) =>
    s.selection ? `${s.selection.feed}:${s.selection.id}` : 'none',
  );
  const globe = useGlobe();
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

  const selectedEntity = useFeeds((s) => s.selectedEntity);
  const isCockpit = followState?.mode === 'cockpit' && followState.status !== 'released';
  const trackedAircraft = useMemo(() => {
    if (!isCockpit || !followState?.target || followState.target.feed !== 'aircraft') return null;
    if (selectedEntity && typeof selectedEntity === 'object' && 'hex' in selectedEntity) {
      const a = selectedEntity as Aircraft;
      if (a.hex === followState.target.id) return a;
    }
    return null;
  }, [isCockpit, followState?.target, selectedEntity]);

  const leftRef = useRef<HTMLDivElement | null>(null);
  useRailFocusReturn(leftRef);

  return (
    <div className={styles.shell}>
      <NavRail className={styles.railArea} />
      <div className={styles.workspace}>
        <main
          id={GLOBE_REGION_ID}
          tabIndex={-1}
          className={styles.stage}
          aria-label="Globe"
          aria-busy={status === 'loading'}
        >
          <div
            ref={hostRef}
            className={`lv-globe-host ${styles.globe}`}
            data-visual-preset={visualPreset}
          />
          <Suspense fallback={null}>
            {scopeMaskEnabled && <ScopeMask enabled={scopeMaskEnabled} />}
            {sonarEnabled && <CyberSonarOverlay visible={sonarEnabled} />}
            {hudEnabled && (
              <HudOverlay
                visible={hudEnabled}
                theme={
                  visualPreset === 'thermal'
                    ? 'flir'
                    : visualPreset === 'nvg'
                      ? 'nvg'
                      : visualPreset === 'crt'
                        ? 'crt'
                        : 'noir'
                }
              />
            )}
            {isCockpit && (
              <CockpitPfd
                callsign={
                  trackedAircraft?.callsign?.trim() ||
                  trackedAircraft?.hex ||
                  followState?.target?.id
                }
                registration={trackedAircraft?.registration ?? undefined}
                typeCode={trackedAircraft?.typeCode ?? undefined}
                headingDeg={followState?.headingDeg ?? trackedAircraft?.trackDeg ?? undefined}
                speedKnots={trackedAircraft?.groundSpeedKt ?? undefined}
                altitudeFt={
                  trackedAircraft?.altitudeBaroFt ?? trackedAircraft?.altitudeGeomFt ?? undefined
                }
                verticalSpeedFpm={trackedAircraft?.verticalRateFpm ?? undefined}
                squawk={trackedAircraft?.squawk ?? undefined}
                theme={
                  visualPreset === 'thermal'
                    ? 'flir'
                    : visualPreset === 'nvg'
                      ? 'nvg'
                      : visualPreset === 'crt'
                        ? 'crt'
                        : 'noir'
                }
                onExit={() => globe?.exitCockpit()}
              />
            )}
          </Suspense>
          {status === 'loading' ? <p className={styles.caption}>Loading globe…</p> : null}
          {status === 'failed' ? (
            <p className={styles.caption} role="alert">
              The globe could not start. Check that WebGL is enabled, then reload.
            </p>
          ) : null}
        </main>
        <ImagerySplitDivider />
        <div className={styles.overlay}>
          <div ref={leftRef} className={styles.left} hidden={collapsed === 'side'}>
            <IslandBoundary
              title={
                sidePanel === 'sources'
                  ? 'Sources'
                  : sidePanel === 'launches'
                    ? 'Rocket launches'
                    : 'Layers'
              }
              resetKey={sidePanel ?? 'none'}
              className={styles.island}
            >
              <SidePanel className={styles.island} />
            </IslandBoundary>
          </div>
          <div className={styles.top}>
            <div className={styles.searchSlot}>
              <SearchBar className={styles.searchBar} />
              <IconButton
                label="Search"
                icon={MagnifyingGlassIcon}
                variant="secondary"
                shortcut="/"
                className={styles.searchButton}
                onClick={() => setPaletteOpen(true)}
              />
            </div>
            <div className={styles.chipSlot}>
              <FollowingChip />
              <FeedStatusChip />
              <Button variant="secondary" size="sm" onClick={() => setBriefingOpen(true)}>
                Export briefing
              </Button>
            </div>
          </div>
          <div className={styles.right} hidden={collapsed === 'inspector'}>
            <IslandBoundary title="Inspector" resetKey={selectionKey} className={styles.inspector}>
              <Inspector className={styles.inspector} />
            </IslandBoundary>
          </div>
          <div className={styles.bottom}>
            <div className={styles.creditsSlot}>
              <CreditsButton />
              <div ref={creditsRef} className={styles.cesiumCredits} />
            </div>
            <MapControls />
          </div>
        </div>
      </div>
      <StatusBar className={styles.statusArea} />
      <CommandPalette />
      <ShortcutSheet />
      <Suspense fallback={null}>
        <BriefingModal />
      </Suspense>
      <LayerAnnouncer />
    </div>
  );
}
