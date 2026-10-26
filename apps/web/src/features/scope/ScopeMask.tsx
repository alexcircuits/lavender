import { useEffect, useRef } from 'react';
import {
  calculateKeyholeGeometry,
  calculateScopeTerminusAlpha,
} from '../../globe/renderers/wasmOptics';
import styles from './scope.module.css';

export interface ScopeMaskProps {
  readonly className?: string | undefined;
  readonly enabled?: boolean | undefined;
  readonly featherRatio?: number | undefined;
  readonly cameraAltitudeM?: number | undefined;
}

/**
 * Tactical circular scope aperture mask.
 * Renders an altitude-adaptive circular vignette over the Cesium canvas:
 * - Inside: 100% transparent aperture
 * - Outside: Feathered dark gradient transitioning to altitude-adaptive terminus alpha
 *   (0.94 above 10 Mm to preserve background starfield, 1.0 below 7 Mm for crisp focus).
 * - Repaints ONLY when dimensions change, feather adjusts, or alpha quantizes to a new step.
 */
export function ScopeMask({
  className,
  enabled = true,
  featherRatio = 0.11,
  cameraAltitudeM = 15_000,
}: ScopeMaskProps) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  const terminusAlpha = calculateScopeTerminusAlpha(cameraAltitudeM);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    if (!enabled) {
      ctx.clearRect(0, 0, canvas.width, canvas.height);
      return;
    }

    const dpr = Math.min(1.5, window.devicePixelRatio || 1);
    const rect = canvas.getBoundingClientRect();
    const w = Math.floor(rect.width * dpr);
    const h = Math.floor(rect.height * dpr);

    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }

    const { cx, cy, radiusPx, featherPx } = calculateKeyholeGeometry(w, h, featherRatio);

    ctx.clearRect(0, 0, w, h);

    // Draw outside dark radial vignette
    const innerR = Math.max(0, radiusPx - featherPx * 0.5);
    const outerR = radiusPx + featherPx * 0.5;

    const grad = ctx.createRadialGradient(cx, cy, innerR, cx, cy, outerR);
    grad.addColorStop(0, 'rgba(5, 5, 8, 0)');
    grad.addColorStop(1, `rgba(5, 5, 8, ${terminusAlpha.toFixed(2)})`);

    // Outer rectangle fill beyond keyhole
    ctx.fillStyle = `rgba(5, 5, 8, ${terminusAlpha.toFixed(2)})`;
    ctx.fillRect(0, 0, w, h);

    // Punch out the keyhole using destination-out or composite
    ctx.save();
    ctx.globalCompositeOperation = 'destination-out';
    ctx.beginPath();
    ctx.arc(cx, cy, innerR, 0, Math.PI * 2);
    ctx.fillStyle = '#000000';
    ctx.fill();

    // Feather ring gradient
    ctx.globalCompositeOperation = 'source-over';
    ctx.beginPath();
    ctx.arc(cx, cy, outerR, 0, Math.PI * 2);
    ctx.arc(cx, cy, innerR, 0, Math.PI * 2, true);
    ctx.fillStyle = grad;
    ctx.fill();
    ctx.restore();

    // Subtle tactical compass border ring & tick marks
    ctx.save();
    ctx.strokeStyle = 'rgba(56, 189, 248, 0.22)';
    ctx.lineWidth = Math.max(1, Math.round(dpr));
    ctx.beginPath();
    ctx.arc(cx, cy, radiusPx, 0, Math.PI * 2);
    ctx.stroke();

    // 4 cardinal tick marks
    const tickLen = 6 * dpr;
    const cardinals = [
      { x: cx, y: cy - radiusPx, dx: 0, dy: tickLen }, // N
      { x: cx + radiusPx, y: cy, dx: -tickLen, dy: 0 }, // E
      { x: cx, y: cy + radiusPx, dx: 0, dy: -tickLen }, // S
      { x: cx - radiusPx, y: cy, dx: tickLen, dy: 0 }, // W
    ];

    ctx.beginPath();
    for (const tick of cardinals) {
      ctx.moveTo(tick.x, tick.y);
      ctx.lineTo(tick.x + tick.dx, tick.y + tick.dy);
    }
    ctx.stroke();
    ctx.restore();
  }, [enabled, featherRatio, terminusAlpha]);

  return (
    <div
      data-testid="scope-mask"
      className={`${styles.container} ${className ?? ''}`}
      aria-hidden="true"
    >
      <canvas ref={canvasRef} className={styles.canvas} />
    </div>
  );
}
