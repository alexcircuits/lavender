import { InfoIcon } from '../design';
import { useSystem, useUi, useWorkspace } from '../runtime';
import styles from './shell.module.css';

/**
 * Attribution is always one click away (product spec §5.6): the names of the providers behind
 * what is on the globe, opening the Sources panel with their full attribution and licences.
 */
export function CreditsButton() {
  const catalog = useSystem((s) => s.catalog);
  const layers = useWorkspace((s) => s.layers);
  const setSidePanel = useUi((s) => s.setSidePanel);

  const ids = new Set(
    (catalog?.layers ?? [])
      .filter((l) => l.availability.state === 'available' && layers[l.feed]?.enabled)
      .flatMap((l) => l.sourceIds),
  );
  const names = (catalog?.sources ?? []).filter((s) => ids.has(s.id)).map((s) => s.name);

  return (
    <button type="button" className={styles.creditsButton} onClick={() => setSidePanel('sources')}>
      <InfoIcon size={12} weight="fill" aria-hidden />
      <span className={styles.creditsText}>
        {names.length > 0 ? `Data: ${names.join(', ')}` : 'Data sources'}
      </span>
      <span className="visually-hidden">, show attribution</span>
    </button>
  );
}
