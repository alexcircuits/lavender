import { Button, XIcon } from '../../design';
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
  readonly onExit?: (() => void) | undefined;
}

/**
 * Primary Flight Display (PFD) Avionics HUD for Cockpit View Mode.
 * Renders airspeed tape, altitude tape, pitch ladder, compass tape, and telemetry.
 */
export function CockpitPfd({
  className,
  callsign = 'AIRCRAFT',
  registration,
  typeCode,
  headingDeg = 0,
  speedKnots = 250,
  altitudeFt = 10000,
  verticalSpeedFpm = 0,
  squawk = '1200',
  theme = 'noir',
  onExit,
}: CockpitPfdProps) {
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
