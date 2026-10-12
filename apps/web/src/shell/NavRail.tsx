import { cx, GearSixIcon, type Icon, IconButton, InfoIcon, StackIcon } from '../design';
import { useUi } from '../runtime';
import type { SidePanel } from '../state/stores';
import { moveFocusWithArrows } from './arrowFocus';
import styles from './shell.module.css';

const ITEMS: ReadonlyArray<{
  readonly panel: SidePanel;
  readonly label: string;
  readonly icon: Icon;
}> = [
  { panel: 'layers', label: 'Layers', icon: StackIcon },
  { panel: 'sources', label: 'Sources', icon: InfoIcon },
];

export interface NavRailProps {
  readonly className?: string | undefined;
}

/**
 * The navigation rail (design-system §6): switches the left island. Pressing the current item
 * closes it, giving the globe the room. A bottom bar on narrow screens.
 */
export function NavRail({ className }: NavRailProps) {
  const current = useUi((s) => s.sidePanel);
  const setSidePanel = useUi((s) => s.setSidePanel);
  return (
    <nav
      aria-label="Workspace"
      className={cx(styles.rail, className)}
      onKeyDown={moveFocusWithArrows}
    >
      <div className={styles.railItems}>
        {ITEMS.map((item) => (
          <IconButton
            key={item.panel}
            label={item.label}
            icon={item.icon}
            pressed={current === item.panel}
            tooltipSide="right"
            className={styles.railButton}
            data-rail-panel={item.panel}
            onClick={() => setSidePanel(current === item.panel ? null : item.panel)}
          />
        ))}
      </div>
      {/* Visible so the layout is stable, but honest: it does nothing yet. */}
      <IconButton
        label="Settings (coming later)"
        icon={GearSixIcon}
        tooltipSide="right"
        aria-disabled="true"
        className={cx(styles.railButton, styles.soon)}
      />
    </nav>
  );
}
