import type { GeoPoint } from '../../contracts';
import { Button, CopyIcon, NavigationArrowIcon, Panel, XIcon } from '../../design';
import type { SubmarineCableItem } from '../../globe';
import { useGlobe, useWorkspace } from '../../runtime';
import { FactSheet } from './FactSheet';
import type { Fact } from './facts';
import { copyCoordinates } from './Inspector';
import styles from './inspector.module.css';

export interface CableCardProps {
  readonly cable: SubmarineCableItem;
  readonly className?: string | undefined;
}

export function cableFacts(cable: SubmarineCableItem): Fact[] {
  const facts: Fact[] = [
    { label: 'Cable ID', value: cable.id, mono: true },
    { label: 'Name', value: cable.name },
    { label: 'Segments', value: String(cable.coordinates.length), mono: true },
  ];

  let totalPoints = 0;
  for (const seg of cable.coordinates) {
    totalPoints += seg.length;
  }
  facts.push({ label: 'Waypoints', value: String(totalPoints), mono: true });

  if (cable.bbox) {
    const b = cable.bbox;
    facts.push({
      label: 'Bounding box',
      value: `${b.west.toFixed(2)}°, ${b.south.toFixed(2)}° to ${b.east.toFixed(2)}°, ${b.north.toFixed(2)}°`,
      mono: true,
    });
  }

  return facts;
}

export function CableCard({ cable, className }: CableCardProps) {
  const globe = useGlobe();
  const select = useWorkspace((s) => s.select);
  const facts = cableFacts(cable);

  const firstPoint = cable.coordinates[0]?.[0];
  const position: GeoPoint | null = firstPoint
    ? { lon: firstPoint[0], lat: firstPoint[1] }
    : cable.bbox
      ? {
          lon: (cable.bbox.west + cable.bbox.east) / 2,
          lat: (cable.bbox.south + cable.bbox.north) / 2,
        }
      : null;

  const handleFlyTo = () => {
    if (position) {
      globe?.flyTo({ kind: 'point', position });
    }
  };

  const handleClose = () => {
    select(null);
  };

  return (
    <Panel title="Submarine Cable" className={className}>
      <div className={styles.summary}>
        <span className={styles.summaryText}>{cable.name}</span>
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
        <strong>Submarine Telecommunications Cable:</strong> TeleGeography Submarine Cable Map data.
        Licensed under Creative Commons Attribution-NonCommercial-ShareAlike 3.0 Unported (CC
        BY-NC-SA 3.0).
        <div className={styles.noticeAttribution}>
          Source:{' '}
          <a
            href="https://www.submarinecablemap.com/"
            target="_blank"
            rel="noopener noreferrer"
            style={{ color: 'var(--ink)' }}
          >
            TeleGeography
          </a>
        </div>
      </div>

      <div className={styles.subhead}>Cable Characteristics</div>
      <FactSheet label="Cable Characteristics" facts={facts} />

      <div className={styles.actions}>
        {position && (
          <Button variant="ghost" onClick={handleFlyTo}>
            <NavigationArrowIcon size={14} />
            Fly to
          </Button>
        )}
        {position && (
          <Button variant="ghost" onClick={() => copyCoordinates(position)}>
            <CopyIcon size={14} />
            Copy coordinates
          </Button>
        )}
      </div>
    </Panel>
  );
}
