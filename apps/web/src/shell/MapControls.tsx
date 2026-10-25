import type { ImageryDescriptor } from '../contracts';
import {
  CompassIcon,
  CopyIcon,
  CrosshairIcon,
  HouseIcon,
  type Icon,
  IconButton,
  MapTrifoldIcon,
  Menu,
  MenuContent,
  MenuGroup,
  MenuGroupLabel,
  MenuItem,
  MenuRadioGroup,
  MenuRadioItem,
  MenuTrigger,
  MinusIcon,
  PlusIcon,
} from '../design';
import { commandById, runCommand, shortcutLabel, useCommandContext } from '../features/commands';
import { CITY_PRESETS } from '../lib/cityPois';
import { useGlobe, useSystem, useWorkspace } from '../runtime';
import { moveFocusWithArrows } from './arrowFocus';
import styles from './shell.module.css';

const CAMERA: ReadonlyArray<{ readonly id: string; readonly icon: Icon }> = [
  { id: 'globe.zoomIn', icon: PlusIcon },
  { id: 'globe.zoomOut', icon: MinusIcon },
  { id: 'globe.resetNorth', icon: CompassIcon },
  { id: 'globe.home', icon: HouseIcon },
];

function imageryDetail(imagery: ImageryDescriptor, sourceName: string | undefined): string {
  if (imagery.kind === 'bundled') return 'Offline';
  return sourceName ?? imagery.sourceId;
}

/**
 * Bottom-right map controls (design-system §6, §9): zoom, north up, home and the base imagery
 * menu. They run the same registry commands as the keyboard, and stay disabled until the
 * globe has mounted.
 */
export function MapControls() {
  const globe = useGlobe();
  const ctx = useCommandContext();
  const catalog = useSystem((s) => s.catalog);
  const chosen = useWorkspace((s) => s.imageryId);
  const setImagery = useWorkspace((s) => s.setImagery);
  const disabled = globe === null;
  const imagery = catalog?.imagery ?? [];
  const active =
    imagery.find((i) => i.id === chosen) ?? imagery.find((i) => i.isDefault) ?? imagery[0];

  return (
    <div
      role="toolbar"
      aria-label="Map controls"
      aria-orientation="vertical"
      className={styles.mapControls}
      onKeyDown={moveFocusWithArrows}
    >
      {CAMERA.map(({ id, icon }) => {
        const command = commandById(id);
        if (!command) return null;
        return (
          <IconButton
            key={id}
            label={command.title}
            icon={icon}
            tooltipSide="left"
            disabled={disabled}
            {...(command.shortcut ? { shortcut: shortcutLabel(command.shortcut) } : {})}
            onClick={() => runCommand(command, ctx)}
          />
        );
      })}
      <IconButton
        label="Copy view link"
        icon={CopyIcon}
        tooltipSide="left"
        disabled={disabled || catalog === null}
        onClick={() => {
          void ctx.runtime.copyViewLink();
        }}
      />
      <Menu>
        <MenuTrigger
          disabled={disabled}
          render={<IconButton label="Quick locations" icon={CrosshairIcon} tooltipSide="left" />}
        />
        <MenuContent side="left" align="end">
          <MenuGroup>
            <MenuGroupLabel>City &amp; landmark presets</MenuGroupLabel>
            {CITY_PRESETS.map((city) => {
              const primaryPoi = city.pois[0];
              return (
                <MenuItem
                  key={city.id}
                  onClick={() => {
                    globe?.flyTo(
                      primaryPoi
                        ? { kind: 'point', position: primaryPoi.position }
                        : { kind: 'bbox', bbox: city.bbox },
                    );
                  }}
                >
                  {city.name}
                  {primaryPoi ? ` · ${primaryPoi.name}` : ''}
                </MenuItem>
              );
            })}
          </MenuGroup>
        </MenuContent>
      </Menu>
      <Menu>
        <MenuTrigger
          disabled={disabled || imagery.length === 0}
          render={<IconButton label="Imagery" icon={MapTrifoldIcon} tooltipSide="left" />}
        />
        <MenuContent side="left" align="end">
          <MenuGroup>
            <MenuGroupLabel>Base imagery</MenuGroupLabel>
            <MenuRadioGroup
              value={active?.id ?? ''}
              onValueChange={(value: string) => setImagery(value)}
            >
              {imagery.map((item) => (
                <MenuRadioItem
                  key={item.id}
                  value={item.id}
                  detail={imageryDetail(
                    item,
                    catalog?.sources.find((s) => s.id === item.sourceId)?.name,
                  )}
                >
                  {item.name}
                </MenuRadioItem>
              ))}
            </MenuRadioGroup>
          </MenuGroup>
        </MenuContent>
      </Menu>
    </div>
  );
}
