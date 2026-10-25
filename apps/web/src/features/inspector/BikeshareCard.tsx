import type { BikeshareStation, GeoPoint } from '../../contracts';
import { Button, CopyIcon, NavigationArrowIcon, Panel, XIcon } from '../../design';
import { useGlobe, useWorkspace } from '../../runtime';
import { FactSheet } from './FactSheet';
import type { Fact } from './facts';
import { copyCoordinates } from './Inspector';
import styles from './inspector.module.css';

export interface BikeshareCardProps {
  readonly station: BikeshareStation;
  readonly className?: string | undefined;
}

export function bikeshareFacts(station: BikeshareStation): Fact[] {
  const facts: Fact[] = [
    { label: 'Station Name', value: station.name },
    { label: 'System', value: station.systemId },
    { label: 'Station ID', value: station.stationId, mono: true },
  ];

  if (station.bikesAvailable !== undefined) {
    facts.push({
      label: 'Bikes available',
      value: String(station.bikesAvailable),
      mono: true,
      tone: station.bikesAvailable > 2 ? 'good' : station.bikesAvailable > 0 ? 'warning' : 'alert',
    });
  }

  if (station.docksAvailable !== undefined) {
    facts.push({
      label: 'Empty docks',
      value: String(station.docksAvailable),
      mono: true,
    });
  }

  if (station.capacity !== undefined) {
    facts.push({
      label: 'Total capacity',
      value: `${station.capacity} docks`,
      mono: true,
    });
  }

  facts.push({
    label: 'Status',
    value: station.isRenting && station.isInstalled ? 'Operational' : 'Unavailable',
    tone: station.isRenting && station.isInstalled ? 'good' : 'alert',
  });

  facts.push({
    label: 'Position',
    value: `${station.position.lat.toFixed(5)}°, ${station.position.lon.toFixed(5)}°`,
    mono: true,
  });

  return facts;
}

export function BikeshareCard({ station, className }: BikeshareCardProps) {
  const globe = useGlobe();
  const select = useWorkspace((s) => s.select);
  const facts = bikeshareFacts(station);
  const position: GeoPoint = station.position;

  const handleFlyTo = () => {
    globe?.flyTo({ kind: 'point', position });
  };

  const handleClose = () => {
    select(null);
  };

  return (
    <Panel title="Bike Share Station" className={className}>
      <div className={styles.summary}>
        <span className={styles.summaryText}>{station.name}</span>
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
        <strong>Bike Share Network (GBFS):</strong> Live dock and vehicle availability data
        published under open data terms by municipal bike share programs.
      </div>

      <div className={styles.subhead}>Station Status</div>
      <FactSheet label="Station Status" facts={facts} />

      <div className={styles.actions}>
        <Button variant="ghost" onClick={handleFlyTo}>
          <NavigationArrowIcon size={14} />
          Fly to
        </Button>
        <Button variant="ghost" onClick={() => copyCoordinates(position)}>
          <CopyIcon size={14} />
          Copy coordinates
        </Button>
      </div>
    </Panel>
  );
}
