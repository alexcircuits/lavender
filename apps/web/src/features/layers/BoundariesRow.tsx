import { memo, useId, useState } from 'react';
import { IconButton, ListRow, MapTrifoldIcon, SlidersHorizontalIcon, Toggle } from '../../design';
import type { BoundaryClassVisibility } from '../../globe';
import { useGlobe, useWorkspace } from '../../runtime';
import { FamilySwatch } from './FamilySwatch';
import styles from './layers.module.css';

export interface BoundariesRowProps {
  readonly onOpenSources?: () => void;
}

/**
 * Boundaries layer row with Natural Earth per-class toggles (international land,
 * disputed lines, breakaway territories, and maritime indicative lines) and
 * transparent point-of-view disclaimer and attribution.
 */
export const BoundariesRow = memo(function BoundariesRow({
  onOpenSources: _onOpenSources,
}: BoundariesRowProps) {
  const [optionsOpen, setOptionsOpen] = useState(false);
  const optionsId = useId();
  const globe = useGlobe();

  const enabled = useWorkspace((s) => s.boundariesEnabled);
  const setEnabled = useWorkspace((s) => s.setBoundariesEnabled);
  const classes = useWorkspace((s) => s.boundaryClasses);
  const setClasses = useWorkspace((s) => s.setBoundaryClasses);

  const toggleClass = (key: keyof BoundaryClassVisibility) => {
    const updated = { ...classes, [key]: !classes[key] };
    setClasses({ [key]: !classes[key] });
    if (enabled) {
      globe?.setBoundaryClassVisibility(updated);
    }
  };

  const handleMasterToggle = (on: boolean) => {
    setEnabled(on);
    if (!on) {
      globe?.setBoundaryClassVisibility({
        internationalLand: false,
        disputedLine: false,
        breakawayArea: false,
        maritimeIndicative: false,
      });
    } else {
      globe?.setBoundaryClassVisibility(classes);
    }
  };

  return (
    <>
      <ListRow
        data-feed="boundaries"
        leading={<FamilySwatch family="infrastructure" icon={MapTrifoldIcon} />}
        title="Boundaries"
        description={<span className={styles.rowText}>Natural Earth · Public domain</span>}
        trailing={
          <>
            <IconButton
              label="Boundaries options"
              icon={SlidersHorizontalIcon}
              size="sm"
              aria-expanded={optionsOpen}
              {...(optionsOpen ? { 'aria-controls': optionsId } : {})}
              onClick={() => setOptionsOpen((open) => !open)}
            />
            <Toggle
              aria-label="Boundaries"
              checked={enabled}
              onCheckedChange={handleMasterToggle}
            />
          </>
        }
        onSelect={() => handleMasterToggle(!enabled)}
      />
      {optionsOpen && (
        <li id={optionsId} className={styles.options}>
          <fieldset aria-label="Boundaries options" className={styles.optionsGroup}>
            <div className={styles.optionStack}>
              <div className={styles.optionRow}>
                <span>International land borders</span>
                <Toggle
                  aria-label="International land borders"
                  checked={classes.internationalLand}
                  onCheckedChange={() => toggleClass('internationalLand')}
                />
              </div>
              <div className={styles.optionRow}>
                <span>Disputed boundary lines</span>
                <Toggle
                  aria-label="Disputed boundary lines"
                  checked={classes.disputedLine}
                  onCheckedChange={() => toggleClass('disputedLine')}
                />
              </div>
              <div className={styles.optionRow}>
                <span>Breakaway &amp; contested areas</span>
                <Toggle
                  aria-label="Breakaway & contested areas"
                  checked={classes.breakawayArea}
                  onCheckedChange={() => toggleClass('breakawayArea')}
                />
              </div>
              <div className={styles.optionRow}>
                <span>Maritime indicative lines</span>
                <Toggle
                  aria-label="Maritime indicative lines"
                  checked={classes.maritimeIndicative}
                  onCheckedChange={() => toggleClass('maritimeIndicative')}
                />
              </div>
              <div className={styles.boundaryNotice} role="note">
                Natural Earth shows de facto boundaries; disputed-area classification represents one
                point of view.
              </div>
              <div className={styles.boundaryAttribution}>
                Made with Natural Earth (public domain)
              </div>
            </div>
          </fieldset>
        </li>
      )}
    </>
  );
});
