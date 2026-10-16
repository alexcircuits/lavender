import type { ReactNode } from 'react';
import {
  CYCLONE_STROKE_PX,
  cycloneSpiral,
  HAZARD_EDGE_PX,
  KEYLINE_PX,
  QUAKE_RING_STROKE_PX,
} from '../../globe/renderers/hazardShapes';
import styles from './layers.module.css';

const WINDS = ['<34 kn', '34–63 kn', '64–95 kn', '96–136 kn', '≥137 kn'];
const QUAKES = ['<M3', 'M3–<4', 'M4–<5', 'M5–<6', '≥M6'];
// FRP is continuous (the inspector shows 49.6 MW), so bins are written "a–<b" like magnitudes.
const FIRES = ['<10 MW', '10–<50 MW', '50–<100 MW', '100–<250 MW', '≥250 MW'];

type LegendKind = 'wind' | 'magnitude' | 'fire';
/** The globe's symbol for each hazard (design-system §4). */
type HazardShape = 'ring' | 'cell' | 'spiral';

const SHAPES: Readonly<Record<LegendKind, HazardShape>> = {
  magnitude: 'ring',
  fire: 'cell',
  wind: 'spiral',
};

/** Swatch box; the symbol inside is drawn at a representative size with its hazard edge. */
const BOX = 18;
const MID = BOX / 2;
const RING_PX = 12;
const CELL_PX = 10;
const SPIRAL_PX = 12;

/**
 * Draws a shape three times like the globe glyphs, widest first: `--map-outline`, the light
 * keyline, then the ramp colour (`currentColor`, set per step by the stylesheet).
 */
function EdgedLayers({ draw }: { readonly draw: (grow: number) => ReactNode }) {
  return (
    <>
      <g className={styles.legendOutline}>{draw(HAZARD_EDGE_PX)}</g>
      <g className={styles.legendKeyline}>{draw(KEYLINE_PX)}</g>
      <g className={styles.legendFill}>{draw(0)}</g>
    </>
  );
}

function Ring() {
  // The stroke's centre line; the edge only grows outwards, the centre stays open.
  const radius = RING_PX / 2 - QUAKE_RING_STROKE_PX / 2;
  return (
    <EdgedLayers
      draw={(grow) => (
        <circle
          cx={MID}
          cy={MID}
          r={radius + grow / 2}
          fill="none"
          strokeWidth={QUAKE_RING_STROKE_PX + grow}
        />
      )}
    />
  );
}

function Cell() {
  return (
    <EdgedLayers
      draw={(grow) => {
        const side = CELL_PX + 2 * grow;
        return <rect x={MID - side / 2} y={MID - side / 2} width={side} height={side} />;
      }}
    />
  );
}

/** The northern-hemisphere spiral from the same geometry as the globe glyph. */
function spiralArmPaths(reachPx: number, centre: number): string[] {
  return cycloneSpiral('north').arms.map((arm) =>
    arm
      .map(
        ([x, y], i) =>
          `${i === 0 ? 'M' : 'L'}${(centre + x * reachPx).toFixed(2)} ${(centre + y * reachPx).toFixed(2)}`,
      )
      .join(' '),
  );
}

function Spiral() {
  const { eyeRadius } = cycloneSpiral('north');
  const arms = spiralArmPaths(SPIRAL_PX, MID);
  return (
    <EdgedLayers
      draw={(grow) => (
        <>
          {arms.map((d) => (
            <path
              key={d}
              d={d}
              fill="none"
              strokeWidth={CYCLONE_STROKE_PX + 2 * grow}
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          ))}
          <circle cx={MID} cy={MID} r={eyeRadius * SPIRAL_PX + grow} stroke="none" />
        </>
      )}
    />
  );
}

function Swatch({
  shape,
  step,
}: {
  readonly shape: HazardShape;
  readonly step: number | 'unknown';
}) {
  return (
    <svg
      className={styles.legendSwatch}
      data-step={step}
      data-shape={shape}
      viewBox={`0 0 ${BOX} ${BOX}`}
      width={BOX}
      height={BOX}
      aria-hidden
      focusable="false"
    >
      {shape === 'ring' ? <Ring /> : shape === 'cell' ? <Cell /> : <Spiral />}
    </svg>
  );
}

export function HazardLegend({ kind }: { readonly kind: LegendKind }) {
  const ariaLabel =
    kind === 'wind'
      ? 'Cyclone sustained-wind legend'
      : kind === 'magnitude'
        ? 'Earthquake magnitude legend'
        : 'Active fire intensity legend';
  const steps = kind === 'wind' ? WINDS : kind === 'magnitude' ? QUAKES : FIRES;
  // Swatches take the symbol's shape: quakes are rings, fires square cells, cyclones spirals.
  const shape = SHAPES[kind];
  return (
    <ul className={styles.legend} aria-label={ariaLabel}>
      {steps.map((label, step) => (
        <li key={label}>
          <Swatch shape={shape} step={step} />
          {label}
        </li>
      ))}
      {kind === 'wind' ? (
        <li>
          <Swatch shape={shape} step="unknown" />
          Wind not reported
        </li>
      ) : null}
    </ul>
  );
}
