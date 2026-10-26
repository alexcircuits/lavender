import type { GeoPoint, GnssCell, GnssLevel } from '../../contracts';
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

export type { GnssCell, GnssLevel } from '../../contracts';

export interface GnssCardProps {
  readonly cell: GnssCell;
  readonly className?: string | undefined;
}

export function gnssInterpretationText(level: GnssLevel): string {
  return level !== 'low' ? 'Suspected GNSS interference' : 'Nominal navigation accuracy';
}

export function gnssFacts(cell: GnssCell): Fact[] {
  const interpretation = gnssInterpretationText(cell.level);
  return [
    { label: '0.5° × 0.5° Cell', value: cell.id, mono: true },
    { label: 'Severity Level', value: cell.level.toUpperCase(), mono: true },
    {
      label: 'Degraded Ratio',
      value: `${cell.degraded} / ${cell.aircraft} aircraft`,
      mono: true,
    },
    {
      label: 'Discounted Anomaly Rate',
      value: `${cell.percentDegraded.toFixed(1)}%`,
      mono: true,
    },
    { label: 'Interpretation', value: interpretation },
    { label: 'Primary State', value: cell.state, mono: true },
    {
      label: 'DO-260A/B Classifier',
      value: 'NIC < 7 or NACp < 8 with 1-aircraft false-positive discount',
    },
    {
      label: 'Cell Center',
      value: `${cell.center.lat.toFixed(4)}°, ${cell.center.lon.toFixed(4)}°`,
      mono: true,
    },
    {
      label: 'Bounding Box',
      value: `${cell.bbox.south.toFixed(2)}°..${cell.bbox.north.toFixed(2)}°, ${cell.bbox.west.toFixed(2)}°..${cell.bbox.east.toFixed(2)}°`,
      mono: true,
    },
  ];
}

export function GnssCard({ cell, className }: GnssCardProps) {
  const globe = useGlobe();
  const select = useWorkspace((s) => s.select);
  const facts = gnssFacts(cell);
  const badgeText = cell.level.toUpperCase();
  const interpretation = gnssInterpretationText(cell.level);

  const position: GeoPoint = {
    lat: cell.center.lat,
    lon: cell.center.lon,
  };

  return (
    <Panel
      title={`GNSS Cell ${cell.id}`}
      className={className}
      data-inspector=""
      actions={<IconButton label="Close inspector" icon={XIcon} onClick={() => select(null)} />}
      toolbar={
        <p className={styles.summary}>
          <Badge tone={cell.level === 'low' ? 'neutral' : 'accent'}>{badgeText}</Badge>
          <span className={styles.summaryText}>0.5° × 0.5° Cell · {interpretation}</span>
        </p>
      }
    >
      <div>
        <div className={styles.metricGrid}>
          <div className={styles.metricCard}>
            <span className={styles.metricLabel}>Anomaly Rate</span>
            <span className={styles.metricValue}>{cell.percentDegraded.toFixed(1)}%</span>
            <span className={styles.metricSub}>1-ac discounted</span>
          </div>
          <div className={styles.metricCard}>
            <span className={styles.metricLabel}>Degraded</span>
            <span className={styles.metricValue}>
              {cell.degraded} / {cell.aircraft}
            </span>
            <span className={styles.metricSub}>aircraft</span>
          </div>
          <div className={styles.metricCard}>
            <span className={styles.metricLabel}>Severity</span>
            <span className={styles.metricValue}>{badgeText}</span>
            <span className={styles.metricSub}>{cell.state}</span>
          </div>
        </div>

        <FactSheet label="GNSS Integrity Facts" facts={facts} />

        <div className={styles.noticeBox}>
          DO-260A/B rule: NIC &lt; 7 or NACp &lt; 8 with 1-aircraft false-positive discount (14 CFR
          91.227 minima &amp; gpsjam.org aggregation).
        </div>
      </div>

      <div className={styles.actions}>
        <Button
          variant="primary"
          icon={NavigationArrowIcon}
          onClick={() => globe?.flyTo({ kind: 'bbox', bbox: cell.bbox })}
        >
          Fly to cell
        </Button>
        <Button variant="ghost" icon={CopyIcon} onClick={() => void copyCoordinates(position)}>
          Copy coordinates
        </Button>
      </div>
    </Panel>
  );
}
