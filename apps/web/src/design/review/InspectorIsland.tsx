import { useState } from 'react';
import { cx } from '../cx';
import {
  AirplaneIcon,
  ArrowSquareOutIcon,
  CopyIcon,
  CrosshairIcon,
  DotsThreeIcon,
  NavigationArrowIcon,
  XIcon,
} from '../icons';
import { Button } from '../primitives/Button';
import { IconButton } from '../primitives/IconButton';
import { List, ListRow } from '../primitives/List';
import { Menu, MenuContent, MenuItem, MenuSeparator, MenuTrigger } from '../primitives/Menu';
import { Panel } from '../primitives/Panel';
import { showToast } from '../primitives/toast-store';
import styles from './DesignReview.module.css';
import { type Fact, INSPECTED_AIRCRAFT, NEARBY_CONTACTS } from './data';
import { FamilySwatch } from './LayersIsland';

function FactSheet({ facts, label }: { facts: ReadonlyArray<Fact>; label: string }) {
  return (
    <dl className={styles.facts} aria-label={label}>
      {facts.map((fact) => (
        <div key={fact.label} className={styles.fact}>
          <dt>{fact.label}</dt>
          <dd className={cx(fact.mono ? 'mono' : 'tabular')}>{fact.value}</dd>
        </div>
      ))}
    </dl>
  );
}

/** The inspector island (design-system §6): header, facts, provenance, actions. */
export function InspectorIsland() {
  const [tracking, setTracking] = useState(false);
  const aircraft = INSPECTED_AIRCRAFT;
  return (
    <Panel
      title={aircraft.callsign}
      className={styles.inspector}
      actions={
        <>
          <Menu>
            <MenuTrigger render={<IconButton label="More actions" icon={DotsThreeIcon} />} />
            <MenuContent align="end">
              <MenuItem icon={CopyIcon} shortcut="⌘L">
                Copy link to this aircraft
              </MenuItem>
              <MenuItem icon={NavigationArrowIcon}>Fly to</MenuItem>
              <MenuSeparator />
              <MenuItem icon={XIcon} danger>
                Hide this aircraft
              </MenuItem>
            </MenuContent>
          </Menu>
          <IconButton label="Close inspector" icon={XIcon} shortcut="Esc" />
        </>
      }
      toolbar={
        <p className={styles.entitySummary}>
          <FamilySwatch family="air" icon={AirplaneIcon} />
          {aircraft.summary}
        </p>
      }
    >
      <FactSheet facts={aircraft.facts} label="Facts" />
      <h3 className={styles.subhead}>Provenance</h3>
      <FactSheet facts={aircraft.provenance} label="Provenance" />
      <div className={styles.actionRow}>
        <Button
          variant="primary"
          icon={CrosshairIcon}
          onClick={() => {
            setTracking((t) => !t);
            showToast({
              kind: 'success',
              title: tracking ? 'Stopped tracking SAS1462' : 'Tracking SAS1462',
            });
          }}
        >
          {tracking ? 'Stop tracking' : 'Track'}
        </Button>
        <Button>Follow</Button>
        <Button
          variant="ghost"
          icon={CopyIcon}
          onClick={() => showToast({ kind: 'success', title: 'Coordinates copied' })}
        >
          Copy coordinates
        </Button>
        <Button variant="ghost" icon={ArrowSquareOutIcon}>
          Open source record
        </Button>
      </div>
    </Panel>
  );
}

/** A selectable entity list: j/k to move, Enter to select. */
export function ContactsIsland() {
  const [selected, setSelected] = useState('SAS1462');
  return (
    <Panel title="Nearby aircraft" dense className={styles.contacts}>
      <List aria-label="Nearby aircraft" selectable>
        {NEARBY_CONTACTS.map((contact) => (
          <ListRow
            key={contact.callsign}
            leading={<FamilySwatch family="air" icon={AirplaneIcon} />}
            title={<span className="mono">{contact.callsign}</span>}
            description={`${contact.type} · ${contact.altitude}`}
            meta={contact.age}
            selected={selected === contact.callsign}
            onSelect={() => setSelected(contact.callsign)}
          />
        ))}
      </List>
    </Panel>
  );
}
