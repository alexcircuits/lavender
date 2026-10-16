/**
 * Keyed diffing for renderers (docs/architecture/frontend.md §4): one entry per id, updated in
 * place, created when new, removed when missing from the next snapshot. Collections are never
 * cleared and rebuilt. A generation stamp marks what a snapshot touched, so a diff allocates
 * nothing per kept entity.
 */

export interface KeyedHandlers<T, E> {
  create(item: T, id: string): E;
  update(entry: E, item: T, id: string): void;
  remove(entry: E, id: string): void;
}

export interface DiffResult {
  readonly added: number;
  readonly updated: number;
  readonly removed: number;
}

interface Slot<E> {
  readonly entry: E;
  stamp: number;
}

export class KeyedEntries<T, E> {
  readonly #slots = new Map<string, Slot<E>>();
  readonly #idOf: (item: T) => string;
  readonly #handlers: KeyedHandlers<T, E>;
  #generation = 0;

  constructor(idOf: (item: T) => string, handlers: KeyedHandlers<T, E>) {
    this.#idOf = idOf;
    this.#handlers = handlers;
  }

  get size(): number {
    return this.#slots.size;
  }

  get(id: string): E | undefined {
    return this.#slots.get(id)?.entry;
  }

  *entries(): IterableIterator<[string, E]> {
    for (const [id, slot] of this.#slots) yield [id, slot.entry];
  }

  /** Reconciles the live entries with `items`. A repeated id updates the same entry again. */
  apply(items: readonly T[]): DiffResult {
    this.#generation += 1;
    const generation = this.#generation;
    let added = 0;
    let updated = 0;
    for (const item of items) {
      const id = this.#idOf(item);
      const slot = this.#slots.get(id);
      if (slot) {
        this.#handlers.update(slot.entry, item, id);
        if (slot.stamp !== generation) updated += 1;
        slot.stamp = generation;
      } else {
        this.#slots.set(id, { entry: this.#handlers.create(item, id), stamp: generation });
        added += 1;
      }
    }
    let removed = 0;
    for (const [id, slot] of this.#slots) {
      if (slot.stamp === generation) continue;
      this.#handlers.remove(slot.entry, id);
      this.#slots.delete(id);
      removed += 1;
    }
    return { added, updated, removed };
  }

  /** Removes every entry through the `remove` handler. */
  clear(): number {
    const count = this.#slots.size;
    for (const [id, slot] of this.#slots) this.#handlers.remove(slot.entry, id);
    this.#slots.clear();
    return count;
  }

  /** Visits every entry without allocating per entry, for passes over every entity. */
  forEach(visit: (entry: E, id: string) => void): void {
    this.#slots.forEach((slot, id) => {
      visit(slot.entry, id);
    });
  }
}
