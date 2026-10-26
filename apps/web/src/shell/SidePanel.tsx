import { lazy, Suspense } from 'react';
import { Button, ErrorState, Panel, Skeleton } from '../design';
import { AnnotationsPanel } from '../features/annotations/AnnotationsPanel';
import { LaunchesPanel } from '../features/launches';
import { LayerExplorer } from '../features/layers';
import { SettingsPanel } from '../features/settings';
import { SourcesPanel } from '../features/sources';

// The Wave B panels are code-split: eager imports here pushed the startup
// bundle over its 250 KiB budget (check-dist enforces it).
const AlertsPanel = lazy(() =>
  import('../features/alerts').then((m) => ({ default: m.AlertsPanel })),
);
const DirectionsPanel = lazy(() =>
  import('../features/directions').then((m) => ({ default: m.DirectionsPanel })),
);
const GeofencesPanel = lazy(() =>
  import('../features/geofences/GeofencesPanel').then((m) => ({ default: m.GeofencesPanel })),
);
const MissionsPanel = lazy(() =>
  import('../features/missions/MissionsPanel').then((m) => ({ default: m.MissionsPanel })),
);
const RecentImageryPanel = lazy(() =>
  import('../features/imagery/RecentImageryPanel').then((m) => ({
    default: m.RecentImageryPanel,
  })),
);
const ReplayPanel = lazy(() =>
  import('../features/replay').then((m) => ({ default: m.ReplayPanel })),
);

import { useRuntime, useSystem, useUi } from '../runtime';
import styles from './shell.module.css';

const SKELETON_ROWS = 4;

function SkeletonRows() {
  return (
    <div className={styles.skeletonRows}>
      {Array.from({ length: SKELETON_ROWS }, (_, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: static placeholders with no identity.
        <div key={i} className={styles.skeletonRow}>
          <Skeleton width="20px" height="20px" shape="block" />
          <Skeleton width={`${60 - i * 8}%`} />
          <Skeleton width="28px" height="16px" shape="pill" />
        </div>
      ))}
    </div>
  );
}

export interface SidePanelProps {
  readonly className?: string | undefined;
}

/**
 * The left island: the layer explorer or the Sources panel (`ui.sidePanel`). While the catalog
 * loads it is a skeleton at the real geometry; if loading fails it says so and offers Retry.
 */
export function SidePanel({ className }: SidePanelProps) {
  const panel = useUi((s) => s.sidePanel);
  const catalog = useSystem((s) => s.catalog);
  const error = useSystem((s) => s.catalogError);
  const runtime = useRuntime();
  if (!panel) return null;
  if (panel === 'annotations') return <AnnotationsPanel className={className} />;
  if (panel === 'alerts')
    return (
      <Suspense
        fallback={
          <Panel title="Alerts" dense className={className}>
            <SkeletonRows />
          </Panel>
        }
      >
        <AlertsPanel className={className} />
      </Suspense>
    );
  if (panel === 'directions')
    return (
      <Suspense
        fallback={
          <Panel title="Directions" dense className={className}>
            <SkeletonRows />
          </Panel>
        }
      >
        <DirectionsPanel className={className} />
      </Suspense>
    );
  if (panel === 'imagery')
    return (
      <Suspense
        fallback={
          <Panel title="Recent imagery" dense className={className}>
            <SkeletonRows />
          </Panel>
        }
      >
        <RecentImageryPanel className={className} />
      </Suspense>
    );
  if (panel === 'geofences')
    return (
      <Suspense
        fallback={
          <Panel title="Geofences" dense className={className}>
            <SkeletonRows />
          </Panel>
        }
      >
        <GeofencesPanel className={className} />
      </Suspense>
    );
  if (panel === 'missions')
    return (
      <Suspense
        fallback={
          <Panel title="Missions" dense className={className}>
            <SkeletonRows />
          </Panel>
        }
      >
        <MissionsPanel className={className} />
      </Suspense>
    );
  if (panel === 'replay')
    return (
      <Suspense
        fallback={
          <Panel title="Replay" dense className={className}>
            <SkeletonRows />
          </Panel>
        }
      >
        <ReplayPanel className={className} />
      </Suspense>
    );
  if (panel === 'settings') return <SettingsPanel className={className} />;
  const title =
    panel === 'layers' ? 'Layers' : panel === 'launches' ? 'Rocket launches' : 'Sources';

  if (!catalog) {
    return (
      <Panel title={title} dense className={className} aria-busy={error ? undefined : true}>
        {error ? (
          <ErrorState
            live
            title={`${title} could not load`}
            action={
              <Button variant="secondary" onClick={() => runtime.reloadCatalog()}>
                Retry
              </Button>
            }
          >
            {error.message} Lavender keeps retrying on its own.
          </ErrorState>
        ) : (
          <SkeletonRows />
        )}
      </Panel>
    );
  }
  if (panel === 'launches') return <LaunchesPanel catalog={catalog} className={className} />;
  return panel === 'layers' ? (
    <LayerExplorer catalog={catalog} className={className} />
  ) : (
    <SourcesPanel catalog={catalog} className={className} />
  );
}
