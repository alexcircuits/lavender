import type { GeoPoint } from '../../contracts';
import { Button, CopyIcon, NavigationArrowIcon, Panel, XIcon } from '../../design';
import type { DivisionRecordItem } from '../../platform';
import { useGlobe, useWorkspace } from '../../runtime';
import { FactSheet } from './FactSheet';
import type { Fact } from './facts';
import { copyCoordinates } from './Inspector';
import styles from './inspector.module.css';

export type { DivisionRecordItem };

export interface DivisionCardProps {
  readonly division: DivisionRecordItem;
  readonly className?: string | undefined;
}

export function divisionFacts(division: DivisionRecordItem): Fact[] {
  const facts: Fact[] = [
    { label: 'Division Name', value: division.name },
    { label: 'Parent Country', value: division.country },
  ];

  if (division.nameEn && division.nameEn !== division.name) {
    facts.push({ label: 'English Name', value: division.nameEn });
  }

  if (division.iso) {
    facts.push({ label: 'ISO 3166-2', value: division.iso, mono: true });
  }

  if (division.kind) {
    facts.push({ label: 'Division Kind', value: division.kind });
  }

  const b = division.bbox;
  facts.push({
    label: 'Bounding box',
    value: `${b.west.toFixed(2)}°, ${b.south.toFixed(2)}° to ${b.east.toFixed(2)}°, ${b.north.toFixed(2)}°`,
    mono: true,
  });

  return facts;
}

export function DivisionCard({ division, className }: DivisionCardProps) {
  const globe = useGlobe();
  const select = useWorkspace((s) => s.select);
  const facts = divisionFacts(division);

  const center: GeoPoint = {
    lon: (division.bbox.west + division.bbox.east) / 2,
    lat: (division.bbox.south + division.bbox.north) / 2,
  };

  const handleFlyTo = () => {
    globe?.flyTo({ kind: 'point', position: center });
  };

  const handleClose = () => {
    select(null);
  };

  return (
    <Panel title="Administrative Division" className={className}>
      <div className={styles.summary}>
        <span className={styles.summaryText}>
          {division.name} • {division.country}
        </span>
        <button
          type="button"
          className={styles.linkButton}
          onClick={handleClose}
          aria-label="Close inspector"
        >
          <XIcon size={14} />
        </button>
      </div>

      <div className={styles.noticeBox}>
        <strong>Administrative Division (Admin-1):</strong> First-level subnational territorial unit
        derived from Natural Earth 10m Vector data (Public domain).
      </div>

      <div className={styles.subhead}>Territorial Profile</div>
      <FactSheet label="Territorial Profile" facts={facts} />

      <div className={styles.actions}>
        <Button variant="ghost" onClick={handleFlyTo}>
          <NavigationArrowIcon size={14} />
          Fly to
        </Button>
        <Button variant="ghost" onClick={() => copyCoordinates(center)}>
          <CopyIcon size={14} />
          Copy center
        </Button>
      </div>
    </Panel>
  );
}
