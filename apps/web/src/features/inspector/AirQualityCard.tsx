import type { AirQualityReading, GeoPoint } from '../../contracts';
import { Button, CopyIcon, IconButton, NavigationArrowIcon, Panel, XIcon } from '../../design';
import { formatUtcTime, parseTime } from '../../lib/time';
import { useGlobe, useWorkspace } from '../../runtime';
import { FactSheet } from './FactSheet';
import type { Fact } from './facts';
import { copyCoordinates } from './Inspector';
import styles from './inspector.module.css';

export interface AirQualityCardProps {
  readonly reading: AirQualityReading;
  readonly className?: string | undefined;
}

export function airQualityBandLabel(band: string): string {
  switch (band) {
    case 'low':
      return 'Low Health Risk (1–3)';
    case 'moderate':
      return 'Moderate Health Risk (4–6)';
    case 'high':
      return 'High Health Risk (7–10)';
    case 'very-high':
      return 'Very High Health Risk (10+)';
    default:
      return band;
  }
}

export function airQualityFacts(reading: AirQualityReading): Fact[] {
  const facts: Fact[] = [
    { label: 'Station Name', value: reading.name },
    { label: 'Index Value', value: `${reading.label} (${reading.scale})`, mono: true },
    { label: 'Health Category', value: airQualityBandLabel(reading.band) },
  ];

  if (reading.risk) {
    facts.push({ label: 'Risk Assessment', value: reading.risk });
  }

  if (reading.note) {
    facts.push({ label: 'Health Advice', value: reading.note });
  }

  if (reading.zone) {
    facts.push({ label: 'Forecast Zone', value: reading.zone.toUpperCase() });
  }

  facts.push({ label: 'Station ID', value: reading.stationId, mono: true });
  facts.push({ label: 'Data Source', value: reading.provider.toUpperCase() });

  const timeMs = parseTime(reading.observedAt);
  if (timeMs !== null) {
    facts.push({ label: 'Observed Time', value: formatUtcTime(timeMs), mono: true });
  }

  facts.push({
    label: 'Coordinates',
    value: `${reading.latitude.toFixed(4)}°, ${reading.longitude.toFixed(4)}°`,
    mono: true,
  });

  return facts;
}

export function AirQualityCard({ reading, className }: AirQualityCardProps) {
  const globe = useGlobe();
  const select = useWorkspace((s) => s.select);
  const facts = airQualityFacts(reading);

  const position: GeoPoint = {
    lat: reading.latitude,
    lon: reading.longitude,
  };

  return (
    <Panel
      title={reading.name}
      className={className}
      data-inspector=""
      actions={<IconButton label="Close inspector" icon={XIcon} onClick={() => select(null)} />}
      toolbar={
        <p className={styles.summary}>
          <span
            style={{
              display: 'inline-block',
              width: 10,
              height: 10,
              borderRadius: '50%',
              backgroundColor: reading.color,
              marginRight: 6,
            }}
            aria-hidden
          />
          <span className={styles.summaryText}>
            AQHI {reading.label} · {airQualityBandLabel(reading.band)}
          </span>
        </p>
      }
    >
      <div className={styles.body}>
        <FactSheet label="Air Quality Facts" facts={facts} />
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
