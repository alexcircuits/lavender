import { lazy, Suspense, useEffect, useRef, useState } from 'react';
import { Button, CopyIcon, IconButton, MagnifyingGlassIcon } from '../design';

// Code-split with the other Wave B panels to hold the startup budget.
const BriefingModal = lazy(() => import('../features/briefing/BriefingModal'));
const ShareModal = lazy(() => import('../features/share/ShareModal'));
const HudOverlay = lazy(() => import('../features/hud/HudOverlay'));
const ScopeMask = lazy(() => import('../features/scope/ScopeMask'));
const CyberSonarOverlay = lazy(() => import('../features/sonar/CyberSonarOverlay'));
const CockpitPfd = lazy(() => import('../features/cockpit/CockpitPfd'));

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
import { useShareRestoration } from '../features/share/useShareRestoration';
import type { MountGlobe, TrackingState } from '../globe';
import { useGlobe, usePreferences, useUi, useWorkspace } from '../runtime';
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
  const setShareOpen = useUi((s) => s.setShareOpen);
  const sidePanel = useUi((s) => s.sidePanel);
  const visualPreset = usePreferences((s) => s.visualPreset);
  const overlayTheme =
    visualPreset === 'thermal'
      ? 'flir'
      : visualPreset === 'nvg'
        ? 'nvg'
        : visualPreset === 'crt'
          ? 'crt'
          : 'noir';
  const hudEnabled = usePreferences((s) => s.hudEnabled);
  const scopeMaskEnabled = usePreferences((s) => s.scopeMaskEnabled);
  const sonarEnabled = usePreferences((s) => s.sonarEnabled);
  const celestialRingEnabled = usePreferences((s) => s.celestialRingEnabled);
  const selectionKey = useWorkspace((s) =>
    s.selection ? `${s.selection.feed}:${s.selection.id}` : 'none',
  );
  const globe = useGlobe();
  useShareRestoration(globe);
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

  // The celestial ring lives inside the globe engine (canvas overlay above the Cesium
  // canvas); the preference just drives it when the globe attaches or the setting flips.
  useEffect(() => {
    globe?.setCelestialRing(celestialRingEnabled);
  }, [globe, celestialRingEnabled]);

  const isCockpit = followState?.mode === 'cockpit' && followState.status !== 'released';

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
            {hudEnabled && <HudOverlay visible={hudEnabled} theme={overlayTheme} />}
            {isCockpit && <CockpitPfd />}
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
              <Button
                variant="secondary"
                size="sm"
                icon={CopyIcon}
                onClick={() => setShareOpen(true)}
              >
                Share
              </Button>
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
        <ShareModal />
      </Suspense>
      <LayerAnnouncer />
    </div>
  );
}
