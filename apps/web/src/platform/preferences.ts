import type { PreferenceStore } from './types';

const NAMESPACE = 'lavender.v1.';

/**
 * Preferences in `localStorage` (web and desktop WebViews both persist it). Every access is
 * guarded: storage can be disabled, full or throw in private modes, and preferences are
 * conveniences, so failures fall back to defaults silently by design.
 */
export class LocalPreferences implements PreferenceStore {
  readonly #storage: Storage | undefined;

  constructor(storage?: Storage) {
    this.#storage = storage ?? LocalPreferences.#defaultStorage();
  }

  static #defaultStorage(): Storage | undefined {
    try {
      return globalThis.localStorage;
    } catch {
      return undefined;
    }
  }

  get<T>(key: string): T | undefined {
    try {
      const raw = this.#storage?.getItem(NAMESPACE + key);
      return raw == null ? undefined : (JSON.parse(raw) as T);
    } catch {
      return undefined;
    }
  }

  set<T>(key: string, value: T): void {
    try {
      this.#storage?.setItem(NAMESPACE + key, JSON.stringify(value));
    } catch {
      // Storage full or unavailable: the preference simply does not persist.
    }
  }

  remove(key: string): void {
    try {
      this.#storage?.removeItem(NAMESPACE + key);
    } catch {
      // As above.
    }
  }
}
