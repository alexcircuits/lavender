/** A set of listeners with unsubscribe handles. One failing listener never stops the rest. */
export class Listeners<T> {
  readonly #listeners = new Set<(value: T) => void>();
  readonly #label: string;

  constructor(label: string) {
    this.#label = label;
  }

  get size(): number {
    return this.#listeners.size;
  }

  /** Registers a listener; the returned function removes it (idempotent). */
  add(listener: (value: T) => void): () => void {
    // Wrap so the same function registered twice gets two independent subscriptions.
    const entry = (value: T) => listener(value);
    this.#listeners.add(entry);
    return () => {
      this.#listeners.delete(entry);
    };
  }

  emit(value: T): void {
    for (const listener of [...this.#listeners]) {
      try {
        listener(value);
      } catch (error) {
        console.error(`Globe ${this.#label} listener failed`, error);
      }
    }
  }

  clear(): void {
    this.#listeners.clear();
  }
}
