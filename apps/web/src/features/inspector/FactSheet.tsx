import { CircleIcon, cx } from '../../design';
import type { Fact } from './facts';
import styles from './inspector.module.css';

export interface FactSheetProps {
  readonly facts: readonly Fact[];
  readonly label: string;
}

/** A two-column label/value grid (design-system §6): labels in ink-2, values mono when IDs. */
export function FactSheet({ facts, label }: FactSheetProps) {
  return (
    <dl className={styles.facts} aria-label={label}>
      {facts.map((fact) => (
        <div key={fact.label} className={styles.fact}>
          <dt>{fact.label}</dt>
          <dd className={cx(fact.mono ? 'mono' : 'tabular')}>
            {fact.tone ? (
              <span className={styles.toned} data-tone={fact.tone}>
                <CircleIcon size={10} weight="fill" aria-hidden />
                {fact.value}
              </span>
            ) : (
              fact.value
            )}
          </dd>
        </div>
      ))}
    </dl>
  );
}
