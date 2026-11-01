import { useMemo } from 'react';
import { useStore } from 'zustand';
import type { Aircraft } from '../../contracts';
import { Button, XIcon } from '../../design';
import type { GlobeController } from '../../globe';
import { type Runtime, useOptionalRuntime } from '../../runtime';
import { CockpitCloudDeck } from './CockpitCloudDeck';
import styles from './cockpit.module.css';

export interface CockpitPfdProps {
  readonly className?: string | undefined;
  readonly callsign?: string | undefined;
  readonly registration?: string | undefined;
  readonly typeCode?: string | undefined;
  readonly headingDeg?: number | undefined;
  readonly speedKnots?: number | undefined;
  readonly altitudeFt?: number | undefined;
  readonly verticalSpeedFpm?: number | undefined;
  readonly squawk?: string | undefined;
  readonly theme?: 'noir' | 'nvg' | 'flir' | 'crt' | undefined;
  readonly latitude?: number | null | undefined;
  readonly longitude?: number | null | undefined;
  readonly onExit?: (() => void) | undefined;
}

interface CockpitPfdInternalProps extends CockpitPfdProps {
  readonly globe?: GlobeController | null | undefined;
  readonly selectedEntity?: unknown;
  readonly visualPreset?: string | undefined;
  readonly cockpitCloudsEnabled?: boolean | undefined;
}

function CockpitPfdPresentational({
  className,
  callsign: callsignProp,
  registration: registrationProp,
  typeCode: typeCodeProp,
  headingDeg: headingDegProp,
  speedKnots: speedKnotsProp,
  altitudeFt: altitudeFtProp,
  verticalSpeedFpm: verticalSpeedFpmProp,
  squawk: squawkProp,
  theme: themeProp,
  latitude: latitudeProp,
  longitude: longitudeProp,
  onExit: onExitProp,
  globe,
  selectedEntity,
  visualPreset = 'normal',
  cockpitCloudsEnabled = false,
}: CockpitPfdInternalProps) {
  const followState = globe?.getFollowState();
  const aircraft = useMemo(() => {
    if (selectedEntity && typeof selectedEntity === 'object' && 'hex' in selectedEntity) {
      return selectedEntity as Aircraft;
    }
    return null;
  }, [selectedEntity]);

  const callsign =
    callsignProp ??
    (aircraft?.callsign?.trim() || aircraft?.hex || followState?.target?.id || 'AIRCRAFT');
  const registration = registrationProp ?? aircraft?.registration ?? undefined;
  const typeCode = typeCodeProp ?? aircraft?.typeCode ?? undefined;
  const headingDeg = headingDegProp ?? followState?.headingDeg ?? aircraft?.trackDeg ?? 0;
  const speedKnots = speedKnotsProp ?? aircraft?.groundSpeedKt ?? 250;
  const altitudeFt =
    altitudeFtProp ?? aircraft?.altitudeBaroFt ?? aircraft?.altitudeGeomFt ?? 10000;
  const verticalSpeedFpm = verticalSpeedFpmProp ?? aircraft?.verticalRateFpm ?? 0;
  const squawk = squawkProp ?? aircraft?.squawk ?? '1200';
  const latitude = latitudeProp ?? aircraft?.position?.lat;
  const longitude = longitudeProp ?? aircraft?.position?.lon;

  const onExit = onExitProp ?? (() => globe?.exitCockpit());

  const theme =
    themeProp ??
    (visualPreset === 'thermal'
      ? 'flir'
      : visualPreset === 'nvg'
        ? 'nvg'
        : visualPreset === 'crt'
          ? 'crt'
          : 'noir');

  const cloudAnchor = useMemo(() => {
    if (latitude == null || longitude == null) return null;
    if (!Number.isFinite(latitude) || !Number.isFinite(longitude)) return null;
    return {
      latitude,
      longitude,
      altitudeM: Math.max(0, altitudeFt * 0.3048),
    };
  }, [latitude, longitude, altitudeFt]);

  const normHeading = Math.round(((headingDeg % 360) + 360) % 360);
  const headingText = normHeading.toString().padStart(3, '0');

  const themeClass =
    theme === 'nvg'
      ? styles.themeNvg
      : theme === 'flir'
        ? styles.themeFlir
        : theme === 'crt'
          ? styles.themeCrt
          : styles.themeNoir;

  // Derive card compass labels around current heading
  const cardinals = [
    { label: 'N', deg: 0 },
    { label: 'E', deg: 90 },
    { label: 'S', deg: 180 },
    { label: 'W', deg: 270 },
  ];

  return (
    <section
      data-testid="cockpit-pfd"
      className={`${styles.container} ${themeClass} ${className ?? ''}`}
      aria-label="Cockpit Flight Display"
    >
      {cockpitCloudsEnabled && <CockpitCloudDeck anchor={cloudAnchor} />}
      {/* Top Heading Tape */}
      <div className={styles.headingTape}>
        <div className={styles.headingDigits}>{headingText}°</div>
        <div className={styles.headingRose}>
          {cardinals.map((c) => (
            <span key={c.label}>
              {c.label} {c.deg.toString().padStart(3, '0')}
            </span>
          ))}
        </div>
      </div>

      {/* Left Airspeed Tape */}
      <div className={styles.speedTape}>
        <div className={styles.tapeValue}>{Math.round(speedKnots)}</div>
        <div className={styles.tapeUnit}>KTS IAS</div>
      </div>

      {/* Right Altitude Tape */}
      <div className={styles.altitudeTape}>
        <div className={styles.tapeValue}>{Math.round(altitudeFt).toLocaleString()}</div>
        <div className={styles.tapeUnit}>FT MSL</div>
      </div>

      {/* Center Pitch Ladder & Horizon */}
      <div className={styles.pitchLadder}>
        <div className={styles.pitchRung}>
          <span className={styles.pitchLabel}>+10</span>
          <span className={styles.pitchLabel}>+10</span>
        </div>
        <div className={styles.horizonLine} />
        <div className={styles.pitchRung}>
          <span className={styles.pitchLabel}>-10</span>
          <span className={styles.pitchLabel}>-10</span>
        </div>

        {/* Center Aircraft Boresight */}
        <div className={styles.boresight}>
          <div className={styles.boresightWingL} />
          <div className={styles.boresightCenter} />
          <div className={styles.boresightWingR} />
        </div>
      </div>

      {/* Bottom Telemetry Strip */}
      <div className={styles.bottomStrip}>
        <div className={styles.statItem}>
          <span className={styles.statKey}>CALL:</span>
          <span className={styles.statVal}>{callsign}</span>
        </div>
        {registration && (
          <div className={styles.statItem}>
            <span className={styles.statKey}>REG:</span>
            <span className={styles.statVal}>{registration}</span>
          </div>
        )}
        {typeCode && (
          <div className={styles.statItem}>
            <span className={styles.statKey}>TYPE:</span>
            <span className={styles.statVal}>{typeCode}</span>
          </div>
        )}
        <div className={styles.statItem}>
          <span className={styles.statKey}>VSI:</span>
          <span className={styles.statVal}>
            {verticalSpeedFpm >= 0 ? `+${verticalSpeedFpm}` : verticalSpeedFpm} FPM
          </span>
        </div>
        <div className={styles.statItem}>
          <span className={styles.statKey}>SQWK:</span>
          <span className={styles.statVal}>{squawk}</span>
        </div>
      </div>

      {/* Eject / Exit Cockpit Button */}
      {onExit && (
        <div className={styles.exitButton}>
          <Button size="sm" variant="secondary" icon={XIcon} onClick={onExit}>
            Exit Cockpit
          </Button>
        </div>
      )}
    </section>
  );
}

function CockpitPfdConnected({
  runtime,
  ...props
}: CockpitPfdProps & { readonly runtime: Runtime }) {
  const globe = runtime.stores.ui.getState().globe;
  const selectedEntity = useStore(runtime.stores.feeds, (s) => s.selectedEntity);
  const visualPreset = useStore(runtime.stores.preferences, (s) => s.visualPreset);
  const cockpitCloudsEnabled = useStore(runtime.stores.preferences, (s) => s.cockpitCloudsEnabled);

  return (
    <CockpitPfdPresentational
      {...props}
      globe={globe}
      selectedEntity={selectedEntity}
      visualPreset={visualPreset}
      cockpitCloudsEnabled={cockpitCloudsEnabled}
    />
  );
}

/**
 * Primary Flight Display (PFD) Avionics HUD for Cockpit View Mode.
 * Renders airspeed tape, altitude tape, pitch ladder, compass tape, and telemetry.
 */
export function CockpitPfd(props: CockpitPfdProps) {
  const runtime = useOptionalRuntime();
  if (runtime) {
    return <CockpitPfdConnected runtime={runtime} {...props} />;
  }
  return <CockpitPfdPresentational {...props} />;
}

export default CockpitPfd;
