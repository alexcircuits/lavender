import type { LayerFamily } from '../../contracts';
import type { Icon } from '../../design';
import { familySwatch } from './families';
import styles from './layers.module.css';

export interface FamilySwatchProps {
  readonly family: LayerFamily;
  readonly icon?: Icon | undefined;
}

/** The family's map colour with the layer's glyph: identity, so Phosphor `fill`. */
export function FamilySwatch({ family, icon: Glyph }: FamilySwatchProps) {
  return (
    <span className={styles.swatch} data-swatch={familySwatch(family)} aria-hidden>
      {Glyph ? <Glyph size={12} weight="fill" /> : null}
    </span>
  );
}
