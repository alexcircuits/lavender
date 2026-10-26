import type { GeoPoint, RadioCategory, RadioStation } from '../../contracts';
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

export interface RadioCardProps {
  readonly station: RadioStation;
  readonly className?: string | undefined;
}

export function radioCategoryLabel(category: RadioCategory): string {
  return category.charAt(0).toUpperCase() + category.slice(1);
}

export function radioFacts(station: RadioStation): Fact[] {
  const facts: Fact[] = [
    { label: 'Station Name', value: station.name },
    { label: 'Category', value: radioCategoryLabel(station.category) },
    { label: 'Country', value: station.countryCode, mono: true },
  ];
  if (station.state) facts.push({ label: 'Region', value: station.state });
  if (station.language) facts.push({ label: 'Language', value: station.language });
  facts.push({
    label: 'Stream',
    value: `${station.codec} · ${station.bitrateKbps} kbps`,
    mono: true,
  });
  if (station.tags.length > 0) facts.push({ label: 'Tags', value: station.tags.join(', ') });
  if (station.homepage) facts.push({ label: 'Homepage', value: station.homepage, mono: true });
  facts.push({ label: 'Station ID', value: station.id, mono: true });
  facts.push({
    label: 'Coordinates',
    value: `${station.position.lat.toFixed(4)}°, ${station.position.lon.toFixed(4)}°`,
    mono: true,
  });
  return facts;
}

export function RadioCard({ station, className }: RadioCardProps) {
  const globe = useGlobe();
  const select = useWorkspace((s) => s.select);
  const facts = radioFacts(station);
  const category = radioCategoryLabel(station.category);
  const position: GeoPoint = { lat: station.position.lat, lon: station.position.lon };

  return (
    <Panel
      title={station.name}
      className={className}
      data-inspector=""
      actions={<IconButton label="Close inspector" icon={XIcon} onClick={() => select(null)} />}
      toolbar={
        <p className={styles.summary}>
          <Badge tone="neutral">{category.toUpperCase()}</Badge>
          <span className={styles.summaryText}>
            {station.countryCode} · {station.codec}
          </span>
        </p>
      }
    >
      <div>
        <div className={styles.metricGrid}>
          <div className={styles.metricCard}>
            <span className={styles.metricLabel}>Bitrate</span>
            <span className={styles.metricValue}>{station.bitrateKbps}</span>
            <span className={styles.metricSub}>kbps {station.codec}</span>
          </div>
          <div className={styles.metricCard}>
            <span className={styles.metricLabel}>Votes</span>
            <span className={styles.metricValue}>{station.votes}</span>
            <span className={styles.metricSub}>community</span>
          </div>
          <div className={styles.metricCard}>
            <span className={styles.metricLabel}>Clicks</span>
            <span className={styles.metricValue}>{station.clickCount}</span>
            <span className={styles.metricSub}>recent tunes</span>
          </div>
        </div>

        <FactSheet label="Radio Station Facts" facts={facts} />
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
