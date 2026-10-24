/**
 * Mission board model (LB-113): analyst kanban cards on a fixed OSINT workflow
 * (Gotham's Target Boards, adapted — no weapons semantics). Cards optionally
 * reference a live entity (feed + id) and/or a drawn geofence. Boards persist
 * on the device through the platform preferences store, validated like
 * annotations so malformed persisted data never renders.
 */

export const MISSION_COLUMNS = [
  'detected',
  'watching',
  'monitoring',
  'analysing',
  'reported',
] as const;

export type MissionColumn = (typeof MISSION_COLUMNS)[number];

export const COLUMN_LABELS: Record<MissionColumn, string> = {
  detected: 'Detected',
  watching: 'Watching',
  monitoring: 'Monitoring',
  analysing: 'Analysing',
  reported: 'Reported',
};

/** The preference key the board persists under. */
export const MISSIONS_PREF_KEY = 'missions';

export const MAX_CARDS = 100;
export const MAX_TITLE_LENGTH = 80;
export const MAX_NOTE_LENGTH = 500;
const ID_PATTERN = /^[a-zA-Z0-9-]{1,64}$/;

export interface MissionCard {
  readonly id: string;
  readonly title: string;
  readonly note: string;
  readonly column: MissionColumn;
  readonly createdAt: string;
  /** Live entity this card tracks, if any. */
  readonly entity: { readonly feed: string; readonly id: string } | null;
  /** Geofence annotation id this card watches, if any. */
  readonly geofenceId: string | null;
}

const isColumn = (value: unknown): value is MissionColumn =>
  typeof value === 'string' && (MISSION_COLUMNS as readonly string[]).includes(value);

/** Parses and validates a persisted board; throws with a plain reason on bad data. */
export function validateMissions(value: unknown): readonly MissionCard[] {
  if (!Array.isArray(value)) throw new Error('Missions must be a list.');
  if (value.length > MAX_CARDS) throw new Error(`Use at most ${MAX_CARDS} cards.`);
  const ids = new Set<string>();
  const cards = value.map((raw) => {
    if (typeof raw !== 'object' || raw === null || Array.isArray(raw))
      throw new Error('Each card must be an object.');
    const c = raw as Record<string, unknown>;
    if (typeof c.id !== 'string' || !ID_PATTERN.test(c.id) || ids.has(c.id))
      throw new Error('Invalid card identity.');
    ids.add(c.id);
    if (typeof c.title !== 'string' || c.title.length === 0 || c.title.length > MAX_TITLE_LENGTH)
      throw new Error('Invalid card title.');
    if (typeof c.note !== 'string' || c.note.length > MAX_NOTE_LENGTH)
      throw new Error('Invalid card note.');
    if (!isColumn(c.column)) throw new Error('Unknown card column.');
    if (typeof c.createdAt !== 'string') throw new Error('Invalid card timestamp.');
    let entity: MissionCard['entity'] = null;
    if (c.entity !== null && c.entity !== undefined) {
      if (
        typeof c.entity !== 'object' ||
        typeof (c.entity as { feed?: unknown }).feed !== 'string' ||
        typeof (c.entity as { id?: unknown }).id !== 'string'
      )
        throw new Error('Invalid card entity link.');
      entity = c.entity as MissionCard['entity'];
    }
    if (c.geofenceId !== null && c.geofenceId !== undefined && typeof c.geofenceId !== 'string')
      throw new Error('Invalid card geofence link.');
    return {
      id: c.id,
      title: c.title,
      note: c.note,
      column: c.column,
      createdAt: c.createdAt,
      entity,
      geofenceId: (c.geofenceId as string | undefined) ?? null,
    } satisfies MissionCard;
  });
  return cards;
}

export function nextColumn(column: MissionColumn): MissionColumn | null {
  const index = MISSION_COLUMNS.indexOf(column);
  return index >= 0 && index < MISSION_COLUMNS.length - 1
    ? (MISSION_COLUMNS[index + 1] ?? null)
    : null;
}

export function previousColumn(column: MissionColumn): MissionColumn | null {
  const index = MISSION_COLUMNS.indexOf(column);
  return index > 0 ? (MISSION_COLUMNS[index - 1] ?? null) : null;
}

/** Where a new card lands: the first column, always. */
export function firstColumn(): MissionColumn {
  return MISSION_COLUMNS[0];
}
