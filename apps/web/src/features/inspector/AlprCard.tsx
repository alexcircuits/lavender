import type { AlprCamera, GeoPoint } from '../../contracts';
import {
  Badge,
  Button,
  CopyIcon,
  IconButton,
  NavigationArrowIcon,
  Panel,
  XIcon,
} from '../../design';
import { useGlobe, useWorkspace } from '../../runtime';
import { FactSheet } from './FactSheet';
import type { Fact } from './facts';
import { copyCoordinates } from './Inspector';
import styles from './inspector.module.css';

export interface AlprCardProps {
  readonly camera: AlprCamera;
  readonly className?: string | undefined;
}

const COMPASS = ['N', 'NE', 'E', 'SE', 'S', 'SW', 'W', 'NW'] as const;

/** Compass facing such as `"NE (45°)"`, or `"Unspecified"` without a valid bearing. */
export function alprFacing(directionDeg: number | undefined): string {
  if (directionDeg === undefined || !Number.isFinite(directionDeg)) return 'Unspecified';
  const deg = ((directionDeg % 360) + 360) % 360;
  return `${COMPASS[Math.round(deg / 45) % 8] ?? 'N'} (${Math.round(deg)}°)`;
}

export function alprFacts(camera: AlprCamera): Fact[] {
  const facts: Fact[] = [{ label: 'Facing', value: alprFacing(camera.directionDeg) }];
  if (camera.operator) facts.push({ label: 'Operator', value: camera.operator });
  if (camera.manufacturer) facts.push({ label: 'Manufacturer', value: camera.manufacturer });
  if (camera.cameraType) facts.push({ label: 'Camera Type', value: camera.cameraType });
  if (camera.zone) facts.push({ label: 'Zone', value: camera.zone });
  if (camera.refCode) facts.push({ label: 'Reference', value: camera.refCode, mono: true });
  if (camera.lastVerified) {
    facts.push({ label: 'Last Verified', value: camera.lastVerified, mono: true });
  }
  facts.push({ label: 'OSM Node', value: String(camera.osmId), mono: true });
  facts.push({ label: 'Data Source', value: camera.source ?? 'OpenStreetMap' });
  facts.push({
    label: 'Coordinates',
    value: `${camera.position.lat.toFixed(4)}°, ${camera.position.lon.toFixed(4)}°`,
    mono: true,
  });
  return facts;
}

export function AlprCard({ camera, className }: AlprCardProps) {
  const globe = useGlobe();
  const select = useWorkspace((s) => s.select);
  const facts = alprFacts(camera);
  const position: GeoPoint = { lat: camera.position.lat, lon: camera.position.lon };
  const facing = alprFacing(camera.directionDeg);

  return (
    <Panel
      title={camera.operator ?? 'ALPR Camera'}
      className={className}
      data-inspector=""
      actions={<IconButton label="Close inspector" icon={XIcon} onClick={() => select(null)} />}
      toolbar={
        <p className={styles.summary}>
          <Badge tone="accent">ALPR</Badge>
          <span className={styles.summaryText}>Community-mapped camera · {facing}</span>
        </p>
      }
    >
      <div>
        <div className={styles.metricGrid}>
          <div className={styles.metricCard}>
            <span className={styles.metricLabel}>Facing</span>
            <span className={styles.metricValue}>{facing}</span>
            <span className={styles.metricSub}>camera direction</span>
          </div>
          <div className={styles.metricCard}>
            <span className={styles.metricLabel}>Type</span>
            <span className={styles.metricValue}>{camera.cameraType ?? 'ALPR'}</span>
            <span className={styles.metricSub}>
              {camera.manufacturer ?? 'manufacturer unknown'}
            </span>
          </div>
        </div>

        <FactSheet label="ALPR Camera Facts" facts={facts} />
      </div>

      <div className={styles.actions}>
        <Button
          variant="primary"
          icon={NavigationArrowIcon}
          onClick={() => globe?.flyTo({ kind: 'point', position })}
        >
          Fly to
        </Button>
        <Button variant="ghost" icon={CopyIcon} onClick={() => void copyCoordinates(position)}>
          Copy coordinates
        </Button>
      </div>
    </Panel>
  );
}
