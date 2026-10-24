import { useEffect, useState } from 'react';
import { Button, IconButton, Panel, TextField, XIcon } from '../../design';
import { usePlatform, useUi, useWorkspace } from '../../runtime';
import styles from '../annotations/annotations.module.css';
import {
  COLUMN_LABELS,
  MISSION_COLUMNS,
  MISSIONS_PREF_KEY,
  type MissionCard,
  type MissionColumn,
  nextColumn,
  previousColumn,
  validateMissions,
} from './model';

/**
 * The analyst's mission board (LB-113): a kanban across the OSINT workflow.
 * Cards can carry a live entity link (added from the current selection) and a
 * geofence reference; the board persists on this device and survives reloads.
 * Malformed persisted data is reported, never rendered.
 */
export function MissionsPanel({ className }: { readonly className?: string | undefined }) {
  const platform = usePlatform();
  const close = useUi((s) => s.setSidePanel);
  const selection = useWorkspace((s) => s.selection);
  const annotations = useWorkspace((s) => s.annotations);
  const [cards, setCards] = useState<readonly MissionCard[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Hydrate once from device preferences; every change writes back.
  useEffect(() => {
    let alive = true;
    try {
      const stored = platform.preferences.get<unknown>(MISSIONS_PREF_KEY);
      if (alive && stored !== undefined) setCards(validateMissions(stored));
      else if (alive) setCards([]);
    } catch (e) {
      if (alive) {
        setCards([]);
        setError(e instanceof Error ? e.message : 'The saved mission board could not be read.');
      }
    }
    return () => {
      alive = false;
    };
  }, [platform]);

  const update = (next: readonly MissionCard[]) => {
    setCards(next);
    try {
      platform.preferences.set(MISSIONS_PREF_KEY, next);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'This change could not be saved.');
    }
  };

  const addCard = (column: MissionColumn, title: string, note: string) => {
    if (!cards) return;
    const card: MissionCard = {
      id: crypto.randomUUID(),
      title,
      note,
      column,
      createdAt: new Date().toISOString(),
      // A live selection at add time links the card to that entity.
      entity: selection ? { feed: selection.feed, id: selection.id } : null,
      geofenceId: null,
    };
    update([...cards, card]);
  };

  const move = (card: MissionCard, to: MissionColumn | null) => {
    if (!cards || !to) return;
    update(cards.map((c) => (c.id === card.id ? { ...c, column: to } : c)));
  };

  const remove = (card: MissionCard) => {
    if (!cards) return;
    update(cards.filter((c) => c.id !== card.id));
  };

  return (
    <Panel
      title="Missions"
      dense
      className={className}
      actions={<IconButton label="Close missions" icon={XIcon} onClick={() => close(null)} />}
    >
      <div className={styles.stack}>
        <p className={styles.help}>
          A kanban across the investigation workflow: Detected → Watching → Monitoring → Analysing →
          Reported. New cards link the entity you have selected; the board stays on this device.
        </p>
        {error ? (
          <p role="alert" className={styles.error}>
            {error}
          </p>
        ) : null}
        {MISSION_COLUMNS.map((column) => {
          const inColumn = (cards ?? []).filter((c) => c.column === column);
          return (
            <section key={column} aria-label={COLUMN_LABELS[column]}>
              <h3>
                {COLUMN_LABELS[column]} · {inColumn.length}
              </h3>
              <ol className={styles.list}>
                {inColumn.map((card) => (
                  <li key={card.id} className={styles.mark}>
                    <h4>{card.title}</h4>
                    {card.note ? <p className={styles.note}>{card.note}</p> : null}
                    {card.entity ? (
                      <p className={styles.help}>
                        Entity: {card.entity.feed}:{card.entity.id}
                      </p>
                    ) : null}
                    {card.geofenceId ? (
                      <p className={styles.help}>Geofence: {card.geofenceId}</p>
                    ) : null}
                    <p className={styles.help}>
                      <time dateTime={card.createdAt}>{card.createdAt}</time>
                    </p>
                    <div className={styles.actions}>
                      {previousColumn(column) ? (
                        <Button
                          variant="secondary"
                          size="sm"
                          onClick={() => move(card, previousColumn(column))}
                        >
                          To {COLUMN_LABELS[previousColumn(column) as MissionColumn]}
                        </Button>
                      ) : null}
                      {nextColumn(column) ? (
                        <Button
                          variant="secondary"
                          size="sm"
                          onClick={() => move(card, nextColumn(column))}
                        >
                          To {COLUMN_LABELS[nextColumn(column) as MissionColumn]}
                        </Button>
                      ) : null}
                      <Button variant="ghost" size="sm" onClick={() => remove(card)}>
                        Delete
                      </Button>
                    </div>
                  </li>
                ))}
              </ol>
              <AddCard column={column} onAdd={addCard} disabled={cards === null} />
            </section>
          );
        })}
        <p className={styles.help}>
          {cards?.length ?? 0} cards · geofences live in the Geofences panel (
          {annotations.filter((a) => a.kind === 'geofence').length} drawn)
        </p>
      </div>
    </Panel>
  );
}

function AddCard({
  column,
  onAdd,
  disabled,
}: {
  readonly column: MissionColumn;
  readonly onAdd: (column: MissionColumn, title: string, note: string) => void;
  readonly disabled: boolean;
}) {
  const [title, setTitle] = useState('');
  const [note, setNote] = useState('');
  return (
    <form
      className={styles.editor}
      onSubmit={(e) => {
        e.preventDefault();
        const trimmed = title.trim();
        if (!trimmed) return;
        onAdd(column, trimmed, note.trim());
        setTitle('');
        setNote('');
      }}
    >
      <TextField
        label={`New card in ${COLUMN_LABELS[column]}`}
        maxLength={80}
        value={title}
        onChange={(e) => setTitle(e.target.value)}
      />
      <TextField
        label="Note (optional)"
        maxLength={500}
        value={note}
        onChange={(e) => setNote(e.target.value)}
      />
      <Button type="submit" variant="secondary" disabled={disabled || !title.trim()}>
        Add card
      </Button>
    </form>
  );
}
