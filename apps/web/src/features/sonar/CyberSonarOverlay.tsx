import styles from './sonar.module.css';

export interface CyberSonarOverlayProps {
  readonly className?: string | undefined;
  readonly visible?: boolean | undefined;
  readonly rings?: number | undefined;
  readonly opacity?: number | undefined;
}

/**
 * Tactical Cyber Sonar Overlay.
 * Renders an animated radial sweep beam, concentric range rings, and radial spokes
 * for tactical contact acquisition and radar/acoustic scanning (NoirSystems styling).
 */
export function CyberSonarOverlay({
  className,
  visible = true,
  rings = 8,
  opacity = 0.85,
}: CyberSonarOverlayProps) {
  if (!visible) return null;

  const ringCount = Math.max(1, Math.min(16, rings));
  const ringElements = Array.from({ length: ringCount }, (_, i) => {
    const scale = (i + 1) / ringCount;
    const inset = `${((1 - scale) * 50).toFixed(2)}%`;
    return (
      <div
        // biome-ignore lint/suspicious/noArrayIndexKey: Fixed index rings
        key={i}
        className={styles.ring}
        style={{ inset }}
      />
    );
  });

  return (
    <div
      data-testid="cyber-sonar-overlay"
      className={`${styles.overlay} ${className ?? ''}`}
      style={{ '--sonar-opacity': opacity } as React.CSSProperties}
      aria-hidden="true"
    >
      <div className={styles.hudSonar}>
        {/* Concentric distance rings */}
        {ringElements}

        {/* 4 Cardinal crosshair spokes */}
        <div className={styles.spoke} />
        <div className={styles.spoke} style={{ transform: 'rotate(90deg)' }} />
        <div className={styles.spoke} style={{ transform: 'rotate(45deg)', opacity: 0.08 }} />
        <div className={styles.spoke} style={{ transform: 'rotate(135deg)', opacity: 0.08 }} />

        {/* Continuous rotating sweep beam */}
        <div className={styles.sweep} />
      </div>
    </div>
  );
}
