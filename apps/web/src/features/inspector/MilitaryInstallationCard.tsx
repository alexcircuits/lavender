import { Button, CopyIcon, NavigationArrowIcon, Panel, XIcon } from '../../design';
import { spaceMgrs, toMgrs } from '../../lib/mgrs';
import type { MilitaryInstallationItem } from '../../platform';
import { useGlobe, useWorkspace } from '../../runtime';
import { FactSheet } from './FactSheet';
import type { Fact } from './facts';
import { copyCoordinates } from './Inspector';
import styles from './inspector.module.css';

export type { MilitaryInstallationItem };

export interface MilitaryInstallationCardProps {
  readonly installation: MilitaryInstallationItem;
  readonly className?: string | undefined;
}

const KIND_LABELS: Readonly<Record<string, string>> = {
  airfield: 'Military Airbase',
  naval_base: 'Naval Base & Station',
  range: 'Weapons & Training Range',
  barracks: 'Barracks & Garrison',
  base: 'Military Base',
  general: 'Defense Facility',
};

export function installationFacts(item: MilitaryInstallationItem): Fact[] {
  const kindLabel = KIND_LABELS[item.kind] ?? 'Defense Facility';
  const facts: Fact[] = [
    { label: 'Facility Name', value: item.name ?? 'Unnamed Military Facility' },
    { label: 'Facility Type', value: kindLabel },
    { label: 'Facility ID', value: item.id, mono: true },
  ];

  if (item.tags.operator) {
    facts.push({ label: 'Operator / Branch', value: item.tags.operator });
  }

  if (item.tags.military) {
    facts.push({ label: 'Military Classification', value: item.tags.military });
  }

  const mgrs = toMgrs(item.position.lat, item.position.lon);
  if (mgrs) {
    facts.push({ label: 'MGRS Grid', value: spaceMgrs(mgrs), mono: true });
  }

  facts.push({
    label: 'Coordinates',
    value: `${item.position.lat.toFixed(5)}°, ${item.position.lon.toFixed(5)}°`,
    mono: true,
  });

  return facts;
}

export function MilitaryInstallationCard({
  installation,
  className,
}: MilitaryInstallationCardProps) {
  const globe = useGlobe();
  const select = useWorkspace((s) => s.select);
  const facts = installationFacts(installation);

  const handleFlyTo = () => {
    globe?.flyTo({
      kind: 'point',
      position: installation.position,
      heightM: 3000,
    });
  };

  const handleClose = () => {
    select(null);
  };

  const title = installation.name ?? KIND_LABELS[installation.kind] ?? 'Defense Facility';

  return (
    <Panel title={title} className={className}>
      <div className={styles.summary}>
        <span className={styles.summaryText}>
          {KIND_LABELS[installation.kind] ?? 'Defense Facility'}
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

      <div className={styles.sensitiveNotice}>
        <strong>Defense & Security Facility:</strong> Features mapped via OpenStreetMap defense and
        government installation surveys. Verify with active aerial reconnaissance.
      </div>

      <FactSheet label="Installation Facts" facts={facts} />

      <div className={styles.actions}>
        <Button variant="primary" icon={NavigationArrowIcon} onClick={handleFlyTo}>
          Fly to
        </Button>
        <Button
          variant="ghost"
          icon={CopyIcon}
          onClick={() => void copyCoordinates(installation.position)}
        >
          Copy coordinates
        </Button>
      </div>
    </Panel>
  );
}
