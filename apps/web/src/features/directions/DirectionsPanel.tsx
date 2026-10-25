import { useState } from 'react';
import type { Route, RouteStep } from '../../contracts';
import {
  Button,
  cx,
  EmptyState,
  ErrorState,
  NavigationArrowIcon,
  Panel,
  SegmentedControl,
  type SegmentedOption,
} from '../../design';
import { useGlobe, usePlatform } from '../../runtime';
import styles from './directions.module.css';

export type RouteProfile = 'car' | 'bike' | 'foot';

const PROFILE_OPTIONS: readonly SegmentedOption<RouteProfile>[] = [
  { value: 'car', label: 'Drive' },
  { value: 'bike', label: 'Cycle' },
  { value: 'foot', label: 'Walk' },
];

export interface DirectionsPanelProps {
  readonly className?: string | undefined;
}

export function formatDistance(distanceM: number): string {
  if (distanceM < 1000) return `${Math.round(distanceM)} m`;
  const km = (distanceM / 1000).toFixed(1);
  const mi = (distanceM * 0.000621371).toFixed(1);
  return `${km} km (${mi} mi)`;
}

export function formatDuration(durationS: number): string {
  if (durationS < 60) return `${Math.round(durationS)} s`;
  const totalMins = Math.round(durationS / 60);
  if (totalMins < 60) return `${totalMins} min`;
  const hours = Math.floor(totalMins / 60);
  const remainingMins = totalMins % 60;
  return `${hours} h ${remainingMins} min`;
}

export function parseCoordinates(input: string): [number, number] | null {
  const parts = input
    .trim()
    .split(/[,;\s]+/)
    .filter(Boolean)
    .map(Number);
  if (parts.length < 2) return null;
  const [lon, lat] = parts;
  if (lon === undefined || lat === undefined) return null;
  if (!Number.isFinite(lon) || !Number.isFinite(lat)) return null;
  if (lon < -180 || lon > 180 || lat < -90 || lat > 90) return null;
  return [lon, lat];
}

export function DirectionsPanel({ className }: DirectionsPanelProps) {
  const platform = usePlatform();
  const globe = useGlobe();

  const [profile, setProfile] = useState<RouteProfile>('car');
  const [originInput, setOriginInput] = useState<string>('13.3888, 52.5170');
  const [destInput, setDestInput] = useState<string>('13.4050, 52.5200');
  const [loading, setLoading] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [route, setRoute] = useState<Route | null>(null);
  const [selectedStep, setSelectedStep] = useState<number | null>(null);

  const handleSetOriginFromView = () => {
    const centre = globe?.centrePosition();
    if (centre) {
      const formatted = `${centre.lon.toFixed(5)}, ${centre.lat.toFixed(5)}`;
      setOriginInput(formatted);
      const parsed = parseCoordinates(formatted);
      const parsedDest = parseCoordinates(destInput);
      globe?.setRouteWaypoints(parsed, parsedDest);
    }
  };

  const handleSetDestFromView = () => {
    const centre = globe?.centrePosition();
    if (centre) {
      const formatted = `${centre.lon.toFixed(5)}, ${centre.lat.toFixed(5)}`;
      setDestInput(formatted);
      const parsedOrigin = parseCoordinates(originInput);
      const parsed = parseCoordinates(formatted);
      globe?.setRouteWaypoints(parsedOrigin, parsed);
    }
  };

  const handleSwap = () => {
    const tmp = originInput;
    setOriginInput(destInput);
    setDestInput(tmp);
    const parsedOrigin = parseCoordinates(destInput);
    const parsedDest = parseCoordinates(tmp);
    globe?.setRouteWaypoints(parsedOrigin, parsedDest);
  };

  const handleCalculateRoute = async () => {
    const originCoords = parseCoordinates(originInput);
    const destCoords = parseCoordinates(destInput);

    if (!originCoords) {
      setError('Invalid Origin coordinates. Use format "lon, lat" between -180..180 and -90..90.');
      return;
    }
    if (!destCoords) {
      setError(
        'Invalid Destination coordinates. Use format "lon, lat" between -180..180 and -90..90.',
      );
      return;
    }

    setLoading(true);
    setError(null);
    globe?.setRouteWaypoints(originCoords, destCoords);

    try {
      const coordsString = `${originCoords[0]},${originCoords[1]};${destCoords[0]},${destCoords[1]}`;
      const result = await platform.calculateRoute(profile, coordsString, true);
      setRoute(result);
      globe?.setRoute(result);
      if (result.geometry.length > 0 && result.geometry[0]) {
        globe?.flyTo({
          kind: 'point',
          position: {
            lon: result.geometry[0][0],
            lat: result.geometry[0][1],
          },
          heightM: 2000,
        });
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to calculate route.';
      setError(message);
      setRoute(null);
      globe?.setRoute(null);
    } finally {
      setLoading(false);
    }
  };

  const handleClear = () => {
    setRoute(null);
    setError(null);
    setSelectedStep(null);
    globe?.setRoute(null);
    globe?.setRouteWaypoints(null, null);
  };

  const handleStepClick = (step: RouteStep) => {
    setSelectedStep(step.index);
    globe?.selectRouteStep(step.index);
    globe?.flyTo({
      kind: 'point',
      position: {
        lon: step.lon,
        lat: step.lat,
      },
      heightM: 1000,
    });
  };

  return (
    <Panel title="Directions" dense className={cx(styles.container, className)}>
      <div className={styles.modeRow}>
        <SegmentedControl
          aria-label="Travel profile"
          options={PROFILE_OPTIONS}
          value={profile}
          onValueChange={(next: RouteProfile) => setProfile(next)}
        />
      </div>

      <div className={styles.inputsGroup}>
        <div className={styles.inputRow}>
          <span className={styles.label}>Origin (A)</span>
          <div className={styles.fieldWithAction}>
            <input
              type="text"
              className={styles.textInput}
              placeholder="lon, lat"
              value={originInput}
              onChange={(e) => setOriginInput(e.target.value)}
              aria-label="Origin coordinates"
            />
            <Button
              variant="secondary"
              size="sm"
              onClick={handleSetOriginFromView}
              title="Set from current map center"
            >
              Centre
            </Button>
          </div>
        </div>

        <div className={styles.inputRow}>
          <span className={styles.label}>Destination (B)</span>
          <div className={styles.fieldWithAction}>
            <input
              type="text"
              className={styles.textInput}
              placeholder="lon, lat"
              value={destInput}
              onChange={(e) => setDestInput(e.target.value)}
              aria-label="Destination coordinates"
            />
            <Button
              variant="secondary"
              size="sm"
              onClick={handleSetDestFromView}
              title="Set from current map center"
            >
              Centre
            </Button>
          </div>
        </div>
      </div>

      <div className={styles.actions}>
        <Button
          variant="primary"
          onClick={handleCalculateRoute}
          disabled={loading}
          icon={NavigationArrowIcon}
        >
          {loading ? 'Routing…' : 'Find Route'}
        </Button>
        <Button variant="secondary" onClick={handleSwap} disabled={loading}>
          Swap
        </Button>
        <Button variant="ghost" onClick={handleClear} disabled={loading && !route}>
          Clear
        </Button>
      </div>

      {error && <ErrorState title="Routing unavailable">{error}</ErrorState>}

      {route && (
        <div className={styles.summaryCard}>
          <div className={styles.statsRow}>
            <div className={styles.stat}>
              <span className={styles.statValue}>{formatDistance(route.distanceM)}</span>
              <span className={styles.statLabel}>Distance</span>
            </div>
            <div className={styles.stat}>
              <span className={styles.statValue}>{formatDuration(route.durationS)}</span>
              <span className={styles.statLabel}>Est. Duration</span>
            </div>
            <div className={styles.stat}>
              <span className={styles.statValue}>{route.steps ? route.steps.length : '0'}</span>
              <span className={styles.statLabel}>Maneuvers</span>
            </div>
          </div>

          {route.steps && route.steps.length > 0 && (
            <ol className={styles.stepsList} aria-label="Turn-by-turn maneuvers">
              {route.steps.map((step) => {
                const isSelected = selectedStep === step.index;
                return (
                  <li key={step.index}>
                    <button
                      type="button"
                      className={cx(styles.stepItem, isSelected && styles.stepItemSelected)}
                      onClick={() => handleStepClick(step)}
                    >
                      <div className={styles.stepIndex}>{step.index + 1}</div>
                      <div className={styles.stepDetails}>
                        <span className={styles.stepInstruction}>{step.instruction}</span>
                        <span className={styles.stepMeta}>
                          {formatDistance(step.distanceM)} · {formatDuration(step.durationS)}
                        </span>
                      </div>
                    </button>
                  </li>
                );
              })}
            </ol>
          )}
        </div>
      )}

      {!route && !loading && !error && (
        <EmptyState icon={NavigationArrowIcon} title="No active route">
          Enter origin and destination coordinates, or use the map centre buttons, to calculate
          turn-by-turn directions.
        </EmptyState>
      )}

      <div className={styles.attribution}>
        Directions powered by OSRM · © OpenStreetMap contributors (ODbL). Paced outbound requests.
      </div>
    </Panel>
  );
}
