import { Button, ErrorState, Panel, Skeleton } from '../design';
import { AnnotationsPanel } from '../features/annotations/AnnotationsPanel';
import { DirectionsPanel } from '../features/directions';
import { GeofencesPanel } from '../features/geofences/GeofencesPanel';
import { LaunchesPanel } from '../features/launches';
import { LayerExplorer } from '../features/layers';
import { MissionsPanel } from '../features/missions/MissionsPanel';
import { SettingsPanel } from '../features/settings';
import { SourcesPanel } from '../features/sources';
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
  if (panel === 'directions') return <DirectionsPanel className={className} />;
  if (panel === 'geofences') return <GeofencesPanel className={className} />;
  if (panel === 'missions') return <MissionsPanel className={className} />;
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
