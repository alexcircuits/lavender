import type { BoundaryClass, BoundaryFeature, GeoPoint } from '../../contracts';
import { Button, CopyIcon, NavigationArrowIcon, Panel, XIcon } from '../../design';
import { useGlobe, useWorkspace } from '../../runtime';
import { FactSheet } from './FactSheet';
import type { Fact } from './facts';
import { copyCoordinates } from './Inspector';
import styles from './inspector.module.css';

export interface BoundaryCardProps {
  readonly feature: BoundaryFeature;
  readonly className?: string | undefined;
}

export function boundaryClassLabel(bClass: BoundaryClass): string {
  switch (bClass) {
    case 'internationalLand':
      return 'International land border';
    case 'disputedLine':
      return 'Disputed boundary line';
    case 'breakawayArea':
      return 'Breakaway / contested area';
    case 'maritimeIndicative':
      return 'Maritime indicative line';
  }
}

export function boundaryFacts(feature: BoundaryFeature): Fact[] {
  const facts: Fact[] = [{ label: 'Classification', value: boundaryClassLabel(feature.class) }];

  if (feature.status) {
    facts.push({ label: 'Status', value: feature.status });
  }

  if (feature.sovereign) {
    facts.push({ label: 'Sovereign', value: feature.sovereign });
  }

  if (feature.claimants && feature.claimants.length > 0) {
    facts.push({ label: 'Claimants', value: feature.claimants.join(', ') });
  }

  if (feature.note) {
    facts.push({ label: 'Note', value: feature.note });
  }

  const b = feature.bbox;
  facts.push({
    label: 'Bounding box',
    value: `${b.west.toFixed(2)}°, ${b.south.toFixed(2)}° to ${b.east.toFixed(2)}°, ${b.north.toFixed(2)}°`,
    mono: true,
  });

  return facts;
}

export function BoundaryCard({ feature, className }: BoundaryCardProps) {
  const globe = useGlobe();
  const select = useWorkspace((s) => s.select);
  const facts = boundaryFacts(feature);

  const centroid: GeoPoint = {
    lon: (feature.bbox.west + feature.bbox.east) / 2,
    lat: (feature.bbox.south + feature.bbox.north) / 2,
  };

  const flyTo = () => {
    globe?.flyTo({
      kind: 'bbox',
      bbox: feature.bbox,
    });
  };

  const title = feature.name || 'Boundary';

  return (
    <Panel
      title={title}
      dense
      className={className}
      toolbar={
        <p className={styles.summary}>
          <span className={styles.summaryText}>{boundaryClassLabel(feature.class)}</span>
        </p>
      }
      actions={
        <Button size="sm" variant="ghost" onClick={() => select(null)}>
          <XIcon aria-label="Close inspector" />
        </Button>
      }
    >
      <FactSheet facts={facts} label="Boundary details" />

      <div className={styles.noticeBox} role="note">
        Natural Earth shows de facto boundaries; disputed-area classification represents one point
        of view.
        <div className={styles.noticeAttribution}>Made with Natural Earth (public domain)</div>
      </div>

      <div className={styles.actions}>
        <Button size="sm" variant="secondary" onClick={flyTo}>
          <NavigationArrowIcon aria-hidden /> Fly to
        </Button>
        <Button size="sm" variant="ghost" onClick={() => copyCoordinates(centroid)}>
          <CopyIcon aria-hidden /> Copy coordinates
        </Button>
      </div>
    </Panel>
  );
}
