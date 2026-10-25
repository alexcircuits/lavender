import { useState } from 'react';
import type { GeoPoint, TransitVehicle } from '../../contracts';
import { Button, CopyIcon, NavigationArrowIcon, Panel, XIcon } from '../../design';
import { useGlobe, usePlatform, useWorkspace } from '../../runtime';
import { FactSheet } from './FactSheet';
import type { Fact } from './facts';
import { copyCoordinates } from './Inspector';
import styles from './inspector.module.css';

export interface TransitCardProps {
  readonly vehicle: TransitVehicle;
  readonly className?: string | undefined;
}

export function transitFacts(vehicle: TransitVehicle): Fact[] {
  const facts: Fact[] = [
    { label: 'Vehicle ID', value: vehicle.vehicleId, mono: true },
    { label: 'Agency Feed', value: vehicle.feedId },
  ];

  if (vehicle.routeId) {
    facts.push({ label: 'Route', value: vehicle.routeId, mono: true });
  }

  if (vehicle.tripId) {
    facts.push({ label: 'Trip ID', value: vehicle.tripId, mono: true });
  }

  if (vehicle.speedMps !== undefined) {
    const kmh = (vehicle.speedMps * 3.6).toFixed(1);
    const mph = (vehicle.speedMps * 2.23694).toFixed(1);
    facts.push({
      label: 'Speed',
      value: `${kmh} km/h (${mph} mph)`,
      mono: true,
    });
  }

  if (vehicle.bearing !== undefined) {
    facts.push({
      label: 'Bearing',
      value: `${vehicle.bearing.toFixed(0)}°`,
      mono: true,
    });
  }

  facts.push({
    label: 'Position',
    value: `${vehicle.position.lat.toFixed(5)}°, ${vehicle.position.lon.toFixed(5)}°`,
    mono: true,
  });

  return facts;
}

export function TransitCard({ vehicle, className }: TransitCardProps) {
  const globe = useGlobe();
  const platform = usePlatform();
  const select = useWorkspace((s) => s.select);
  const [loadingTrail, setLoadingTrail] = useState(false);
  const [trailLoaded, setTrailLoaded] = useState(false);

  const facts = transitFacts(vehicle);
  const position: GeoPoint = vehicle.position;

  const handleFlyTo = () => {
    globe?.flyTo({ kind: 'point', position });
  };

  const handleClose = () => {
    select(null);
  };

  const handleShowTrail = async () => {
    try {
      setLoadingTrail(true);
      const trail = await platform.transitTrail(vehicle.routeId ?? '', vehicle.id);
      globe?.showTransitTrail(trail);
      setTrailLoaded(true);
    } catch {
      // ignored
    } finally {
      setLoadingTrail(false);
    }
  };

  return (
    <Panel title="Public Transit Vehicle" className={className}>
      <div className={styles.summary}>
        <span className={styles.summaryText}>
          {vehicle.routeId ? `Route ${vehicle.routeId} • ` : ''}Vehicle {vehicle.vehicleId}
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
        <strong>Live Public Transit (GTFS-RT):</strong> Real-time vehicle location streamed from
        regional transit operators under open data terms.
      </div>

      <div className={styles.subhead}>Vehicle Telemetry</div>
      <FactSheet label="Vehicle Telemetry" facts={facts} />

      <div className={styles.actions}>
        <Button variant="ghost" onClick={handleFlyTo}>
          <NavigationArrowIcon size={14} />
          Fly to
        </Button>
        <Button variant="ghost" onClick={() => copyCoordinates(position)}>
          <CopyIcon size={14} />
          Copy coordinates
        </Button>
        <Button
          variant="secondary"
          onClick={handleShowTrail}
          disabled={loadingTrail || trailLoaded}
        >
          {loadingTrail
            ? 'Loading trail...'
            : trailLoaded
              ? 'Trail visible'
              : 'Show trail (WASM RDP)'}
        </Button>
      </div>
    </Panel>
  );
}
