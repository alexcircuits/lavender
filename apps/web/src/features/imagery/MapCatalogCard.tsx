import { useMapCatalog } from '../scenes/api';
import { plainCredit } from '../scenes/model';
import styles from '../scenes/scenes.module.css';

/** "Map sources": base map stacks from GET /api/v1/maps/catalog with availability and credit. */
export function MapCatalogCard({ className }: { readonly className?: string | undefined }) {
  const { data, error, pending } = useMapCatalog();
  return (
    <section className={className} aria-label="Map sources">
      <h3 className={styles.kicker}>Map sources</h3>
      {pending ? <div className={styles.card}>Loading map sources…</div> : null}
      {error ? (
        <div className={styles.card} role="alert">
          Could not load map sources: {error}
        </div>
      ) : null}
      <ul className={styles.shots}>
        {data?.stacks.map((s) => (
          <li key={s.id} className={styles.card}>
            <span className={styles.title}>{s.label}</span>
            <span className={styles.meta}>
              {s.available
                ? 'Available'
                : `Unavailable${s.unavailableReason ? `: ${s.unavailableReason}` : ''}`}
            </span>
            <span className={styles.meta}>{plainCredit(s.creditHtml)}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}
