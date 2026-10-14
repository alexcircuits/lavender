import styles from './layers.module.css';

const WINDS = ['<34 kn', '34–63 kn', '64–95 kn', '96–136 kn', '≥137 kn'];
const QUAKES = ['<M3', 'M3–<4', 'M4–<5', 'M5–<6', '≥M6'];
export function HazardLegend({ kind }: { readonly kind: 'wind' | 'magnitude' }) {
  return (
    <ul
      className={styles.legend}
      aria-label={kind === 'wind' ? 'Cyclone sustained-wind legend' : 'Earthquake magnitude legend'}
    >
      {(kind === 'wind' ? WINDS : QUAKES).map((label, step) => (
        <li key={label}>
          <span className={styles.legendSwatch} data-step={step} aria-hidden />
          {label}
        </li>
      ))}
    </ul>
  );
}
