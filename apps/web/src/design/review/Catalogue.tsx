import { useState } from 'react';
import {
  ArrowClockwiseIcon,
  CaretDownIcon,
  CompassIcon,
  CrosshairIcon,
  GlobeHemisphereEastIcon,
  HouseIcon,
  KeyIcon,
  MapTrifoldIcon,
  MinusIcon,
  PlusIcon,
  StackIcon,
  TrashIcon,
} from '../icons';
import { Button, type ButtonSize, type ButtonVariant } from '../primitives/Button';
import { Dialog, DialogClose } from '../primitives/Dialog';
import { EmptyState } from '../primitives/EmptyState';
import { ErrorState } from '../primitives/ErrorState';
import { FreshnessMeter } from '../primitives/FreshnessMeter';
import { IconButton } from '../primitives/IconButton';
import { Kbd } from '../primitives/Kbd';
import {
  Menu,
  MenuCheckboxItem,
  MenuContent,
  MenuGroup,
  MenuGroupLabel,
  MenuRadioGroup,
  MenuRadioItem,
  MenuSeparator,
  MenuTrigger,
} from '../primitives/Menu';
import { Panel } from '../primitives/Panel';
import { Popover, PopoverContent, PopoverTrigger } from '../primitives/Popover';
import { SearchField } from '../primitives/SearchField';
import { SegmentedControl } from '../primitives/SegmentedControl';
import { Skeleton } from '../primitives/Skeleton';
import { Badge, type FeedStatus, StatusPill } from '../primitives/StatusPill';
import { TextField } from '../primitives/TextField';
import { Toggle } from '../primitives/Toggle';
import { Tooltip } from '../primitives/Tooltip';
import { showToast } from '../primitives/toast-store';
import styles from './DesignReview.module.css';

const VARIANTS: ReadonlyArray<{ variant: ButtonVariant; name: string; label: string }> = [
  { variant: 'primary', name: 'Primary', label: 'Track' },
  { variant: 'secondary', name: 'Secondary', label: 'Follow' },
  { variant: 'ghost', name: 'Ghost', label: 'Copy link' },
  { variant: 'danger', name: 'Danger', label: 'Remove key' },
];
const SIZES: ReadonlyArray<{ size: ButtonSize; label: string }> = [
  { size: 'sm', label: '28' },
  { size: 'md', label: '32' },
  { size: 'lg', label: '36' },
];

export function ButtonsIsland() {
  return (
    <Panel title="Buttons" className={styles.wide}>
      <div className={styles.matrix}>
        <span />
        {SIZES.map(({ size, label }) => (
          <span key={size} className={styles.caption}>
            {label} px
          </span>
        ))}
        <span className={styles.caption}>Disabled</span>
        {VARIANTS.map(({ variant, name, label }) => (
          <div key={variant} className={styles.matrixRow}>
            <span className={styles.caption}>{name}</span>
            {SIZES.map(({ size }) => (
              <Button
                key={size}
                variant={variant}
                size={size}
                {...(variant === 'primary' ? { icon: CrosshairIcon } : {})}
              >
                {label}
              </Button>
            ))}
            <Button variant={variant} disabled>
              {label}
            </Button>
          </div>
        ))}
      </div>
      <div className={styles.row}>
        <span className={styles.caption}>Dialog primary, 40 px</span>
        <Button variant="primary" size="xl">
          Save key
        </Button>
        <Button variant="danger-fill" size="xl">
          Remove key
        </Button>
      </div>
    </Panel>
  );
}

export function ControlsIsland() {
  const [aircraft, setAircraft] = useState(true);
  const [mode, setMode] = useState<'globe' | 'flat' | 'columbus'>('globe');
  const [units, setUnits] = useState<'km' | 'nm' | 'mi'>('km');
  const [following, setFollowing] = useState(false);
  return (
    <Panel title="Controls" className={styles.medium}>
      <div className={styles.stack}>
        <div className={styles.row}>
          <Toggle aria-label="Aircraft" checked={aircraft} onCheckedChange={setAircraft} />
          <span>Aircraft {aircraft ? 'on' : 'off'}</span>
          <Toggle aria-label="Satellites" defaultChecked={false} />
          <span>Off</span>
          <Toggle aria-label="Fires" disabled />
          <span className={styles.muted}>Disabled</span>
          <Toggle aria-label="Vessels" disabled defaultChecked />
          <span className={styles.muted}>Disabled on</span>
        </div>
        {/* Rows keep segmented controls at their intrinsic width. */}
        <div className={styles.row}>
          <SegmentedControl
            aria-label="Globe mode"
            value={mode}
            onValueChange={setMode}
            options={[
              { value: 'globe', label: '3D globe', icon: GlobeHemisphereEastIcon },
              { value: 'flat', label: '2D map', icon: MapTrifoldIcon },
              { value: 'columbus', label: 'Columbus' },
            ]}
          />
          <SegmentedControl
            aria-label="Distance units"
            size="sm"
            value={units}
            onValueChange={setUnits}
            options={[
              { value: 'km', label: 'km' },
              { value: 'nm', label: 'nm' },
              { value: 'mi', label: 'mi' },
            ]}
          />
        </div>
        <div className={styles.row}>
          <div className={styles.mapControls} role="toolbar" aria-label="Map controls">
            <IconButton label="Zoom in" icon={PlusIcon} shortcut="+" tooltipSide="left" />
            <IconButton label="Zoom out" icon={MinusIcon} shortcut="−" tooltipSide="left" />
            <IconButton label="North up" icon={CompassIcon} shortcut="N" tooltipSide="left" />
            <IconButton label="Home" icon={HouseIcon} shortcut="H" tooltipSide="left" />
          </div>
          <div className={styles.stack}>
            <div className={styles.row}>
              <IconButton label="Layers" icon={StackIcon} variant="secondary" />
              <IconButton
                label={following ? 'Stop following' : 'Follow selection'}
                icon={CrosshairIcon}
                shortcut="F"
                pressed={following}
                onClick={() => setFollowing((f) => !f)}
              />
              <IconButton label="Zoom in" icon={PlusIcon} size="sm" />
              <IconButton label="Zoom in" icon={PlusIcon} size="lg" />
              <IconButton label="Remove" icon={TrashIcon} disabled />
            </div>
            <div className={styles.row}>
              <Kbd>⌘K</Kbd>
              <Kbd>/</Kbd>
              <Kbd>J</Kbd>
              <Kbd>K</Kbd>
              <Kbd>Esc</Kbd>
              <span className={styles.caption}>Key hints</span>
            </div>
          </div>
        </div>
      </div>
    </Panel>
  );
}

export function FieldsIsland() {
  return (
    <Panel title="Fields" className={styles.medium}>
      <div className={styles.stack}>
        <SearchField
          label="Search places, coordinates and callsigns"
          placeholder="Search places, coordinates, callsigns"
          shortcutHint="⌘K"
        />
        <TextField
          label="Display name"
          defaultValue="Baltic watch"
          description="Shown on shared links and exports."
        />
        <TextField
          label="FIRMS map key"
          mono
          defaultValue="3f9c2a71e0b84d5c9a6e1f27b4d03c8"
          error="That key has 31 characters; FIRMS map keys have 32. Paste it again."
          leading={<KeyIcon size={16} weight="bold" aria-hidden />}
        />
        <TextField label="Imagery server" disabled defaultValue="Managed by your administrator" />
      </div>
    </Panel>
  );
}

export function OverlaysIsland() {
  const [imagery, setImagery] = useState('satellite');
  const [labels, setLabels] = useState(true);
  const [borders, setBorders] = useState(false);
  return (
    <Panel title="Menus, popovers and dialogs" className={styles.medium}>
      <div className={styles.stack}>
        <div className={styles.row}>
          <Menu>
            <MenuTrigger
              render={
                <Button icon={MapTrifoldIcon} iconEnd={CaretDownIcon}>
                  Imagery
                </Button>
              }
            />
            <MenuContent>
              <MenuGroup>
                <MenuGroupLabel>Base imagery</MenuGroupLabel>
                <MenuRadioGroup value={imagery} onValueChange={(v: string) => setImagery(v)}>
                  <MenuRadioItem value="satellite" detail="EOX">
                    Satellite
                  </MenuRadioItem>
                  <MenuRadioItem value="streets" detail="OpenStreetMap">
                    Dark streets
                  </MenuRadioItem>
                  <MenuRadioItem value="offline" detail="Bundled">
                    Natural Earth (offline)
                  </MenuRadioItem>
                </MenuRadioGroup>
              </MenuGroup>
              <MenuSeparator />
              <MenuCheckboxItem checked={labels} onCheckedChange={setLabels} shortcut="L">
                Place labels
              </MenuCheckboxItem>
              <MenuCheckboxItem checked={borders} onCheckedChange={setBorders}>
                Country borders
              </MenuCheckboxItem>
            </MenuContent>
          </Menu>

          <Popover>
            <PopoverTrigger
              render={
                <Button variant="ghost">
                  <span className={styles.feedChip}>
                    Feeds
                    <StatusPill status="degraded" detail="1" />
                  </span>
                </Button>
              }
            />
            <PopoverContent
              title="Feed health"
              description="4 feeds live, 1 stale, 1 unavailable. Last good data stays on the globe."
            >
              <ul className={styles.healthList}>
                <li>
                  <span>Aircraft</span>
                  <StatusPill status="live" />
                </li>
                <li>
                  <span>Satellites</span>
                  <StatusPill status="stale" detail="4 min" />
                </li>
                <li>
                  <span>Tropical cyclones</span>
                  <StatusPill status="unavailable" />
                </li>
              </ul>
            </PopoverContent>
          </Popover>

          <Tooltip content="Copies a link to this view" shortcut="⌘L">
            <Button variant="ghost">Share view</Button>
          </Tooltip>
        </div>
        <div className={styles.row}>
          <Dialog
            trigger={<Button icon={KeyIcon}>Add a FIRMS key</Button>}
            title="Add a FIRMS key"
            description="Active fires need a free NASA FIRMS map key. It is stored on this device only."
            actions={
              <>
                <DialogClose render={<Button variant="secondary">Cancel</Button>} />
                <DialogClose
                  render={
                    <Button
                      variant="primary"
                      onClick={() => showToast({ kind: 'success', title: 'Key saved' })}
                    >
                      Save key
                    </Button>
                  }
                />
              </>
            }
          >
            <TextField label="Map key" mono placeholder="32 characters" />
          </Dialog>
          <Dialog
            destructive
            trigger={
              <Button variant="danger" icon={TrashIcon}>
                Remove key
              </Button>
            }
            title="Remove the FIRMS key?"
            description="Active fires will stop updating on this device until you add a key again."
            actions={
              <>
                <DialogClose render={<Button variant="secondary">Keep key</Button>} />
                <DialogClose
                  render={
                    <Button
                      variant="danger-fill"
                      onClick={() => showToast({ kind: 'info', title: 'Key removed' })}
                    >
                      Remove key
                    </Button>
                  }
                />
              </>
            }
          />
        </div>
      </div>
    </Panel>
  );
}

const STATUSES: ReadonlyArray<{ status: FeedStatus; detail?: string }> = [
  { status: 'live' },
  { status: 'stale', detail: '3 min' },
  { status: 'degraded' },
  { status: 'unavailable' },
  { status: 'not-configured' },
  { status: 'licence' },
];

export function StatusIsland() {
  return (
    <Panel title="Status and freshness" className={styles.medium}>
      <div className={styles.stack}>
        <div className={styles.row}>
          {STATUSES.map(({ status, detail }) => (
            <StatusPill key={status} status={status} {...(detail ? { detail } : {})} />
          ))}
        </div>
        <div className={styles.row}>
          <Badge>12 new</Badge>
          <Badge tone="accent">Tracking 3</Badge>
        </div>
        <dl className={styles.meterList}>
          <div>
            <dt>Aircraft · 4 s of 10 s</dt>
            <dd>
              <FreshnessMeter label="Aircraft" ageSeconds={4} cadenceSeconds={10} />
            </dd>
          </div>
          <div>
            <dt>Earthquakes · 95 s of 60 s</dt>
            <dd>
              <FreshnessMeter label="Earthquakes" ageSeconds={95} cadenceSeconds={60} />
            </dd>
          </div>
          <div>
            <dt>Satellites · 4 min past 2× cadence</dt>
            <dd>
              <FreshnessMeter label="Satellites" ageSeconds={250} cadenceSeconds={120} />
            </dd>
          </div>
          <div>
            <dt>Tropical cyclones · unknown</dt>
            <dd>
              <FreshnessMeter label="Tropical cyclones" ageSeconds={null} cadenceSeconds={300} />
            </dd>
          </div>
        </dl>
      </div>
    </Panel>
  );
}

export function ToastsIsland() {
  return (
    <Panel title="Toasts" className={styles.medium}>
      <p className={styles.note}>
        Bottom centre, above the status bar. Two at most; info and success leave after 4 s, errors
        stay until acted on.
      </p>
      <div className={styles.row}>
        <Button onClick={() => showToast({ kind: 'success', title: 'Link copied' })}>
          Copy link
        </Button>
        <Button
          onClick={() => showToast({ title: 'Aircraft turned on', description: '12 486 contacts' })}
        >
          Turn on aircraft
        </Button>
        <Button
          variant="danger"
          onClick={() =>
            showToast({
              kind: 'error',
              title: 'Earthquakes are unavailable',
              description: 'USGS has not answered for 6 min. The last good data stays.',
              action: { label: 'Retry', onAction: () => undefined },
            })
          }
        >
          Show an error
        </Button>
      </div>
    </Panel>
  );
}

export function LoadingIsland() {
  return (
    <Panel title="Loading" dense className={styles.narrow}>
      <div className={styles.skeletonRows} aria-busy="true">
        <span className="visually-hidden" role="status">
          Loading layers…
        </span>
        {['72%', '54%', '64%', '48%'].map((width) => (
          <div key={width} className={styles.skeletonRow}>
            <Skeleton width="20px" height="20px" shape="block" />
            <Skeleton width={width} />
            <Skeleton width="28px" shape="pill" />
          </div>
        ))}
      </div>
    </Panel>
  );
}

export function EmptyIsland() {
  return (
    <Panel title="Layers" dense className={styles.narrow}>
      <EmptyState
        icon={StackIcon}
        title="No layers are on"
        action={
          <Button variant="secondary" size="sm" icon={StackIcon}>
            Browse layers
          </Button>
        }
      >
        Turn on a layer to see live data on the globe.
      </EmptyState>
    </Panel>
  );
}

export function ErrorIsland() {
  return (
    <Panel title="Earthquakes" dense className={styles.narrow}>
      <ErrorState
        title="Earthquakes are unavailable"
        action={
          <Button variant="secondary" size="sm" icon={ArrowClockwiseIcon}>
            Retry
          </Button>
        }
      >
        USGS has not answered for 6 min. Lavender keeps the last good data and retries every minute.
      </ErrorState>
    </Panel>
  );
}

export function WarningIsland() {
  return (
    <Panel title="Search" dense className={styles.narrow}>
      <ErrorState severity="warning" title="Place search is unavailable">
        Coordinates and live entities still work.
      </ErrorState>
    </Panel>
  );
}
