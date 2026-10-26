import { BookOpenIcon } from '@phosphor-icons/react/dist/csr/BookOpen';
import { PencilSimpleIcon } from '@phosphor-icons/react/dist/csr/PencilSimple';
import {
  CircleHalfIcon,
  ClockCounterClockwiseIcon,
  CompassIcon,
  cx,
  GearSixIcon,
  type Icon,
  IconButton,
  InfoIcon,
  NavigationArrowIcon,
  PathIcon,
  SquareIcon,
  StackIcon,
  Tooltip,
  WarningCircleIcon,
} from '../design';
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
  { panel: 'directions', label: 'Directions', icon: NavigationArrowIcon },
  { panel: 'imagery', label: 'Recent imagery', icon: CircleHalfIcon },
  { panel: 'replay', label: 'Timeline replay', icon: ClockCounterClockwiseIcon },
  { panel: 'sources', label: 'Sources', icon: InfoIcon },
  { panel: 'annotations', label: 'Annotations', icon: PencilSimpleIcon },
  { panel: 'geofences', label: 'Geofences', icon: PathIcon },
  { panel: 'alerts', label: 'Alert inbox', icon: WarningCircleIcon },
  { panel: 'missions', label: 'Missions', icon: SquareIcon },
  { panel: 'scenes', label: 'Evidence tours', icon: CompassIcon },
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
      <div className={styles.railItems}>
        <Tooltip content="Developer documentation" side="right">
          <a
            href="/docs/"
            target="_blank"
            rel="noreferrer"
            aria-label="Developer documentation"
            className={cx(styles.railButton, styles.documentationLink)}
          >
            <BookOpenIcon size={18} weight="bold" aria-hidden />
          </a>
        </Tooltip>
        <IconButton
          label="Settings"
          icon={GearSixIcon}
          pressed={current === 'settings'}
          tooltipSide="right"
          className={styles.railButton}
          data-rail-panel="settings"
          onClick={() => setSidePanel(current === 'settings' ? null : 'settings')}
        />
      </div>
    </nav>
  );
}
