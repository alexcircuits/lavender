import { useCallback, useEffect, useState } from 'react';
import type { SavedSearch, SavedSearchMatch } from '../../contracts';
import { Button, Toggle } from '../../design';
import { usePlatform, useWorkspace } from '../../runtime';
import styles from './alerts.module.css';

/**
 * Saved Searches & Persistent Feed Monitor tab (LB-140 Item 5):
 * Allows creating, listing, and deleting persistent saved search monitors,
 * inspecting matched entities, and focusing matched entities on the workspace.
 */
export function SavedSearchesTab() {
  const platform = usePlatform();
  const select = useWorkspace((s) => s.select);

  const [searches, setSearches] = useState<readonly SavedSearch[]>([]);
  const [name, setName] = useState('');
  const [query, setQuery] = useState('');
  const [notifyOnMatch, setNotifyOnMatch] = useState(true);
  const [matchesBySearch, setMatchesBySearch] = useState<
    Readonly<Record<string, readonly SavedSearchMatch[]>>
  >({});

  useEffect(() => {
    let active = true;
    void (async () => {
      try {
        const list = await platform.listSavedSearches?.();
        if (active && list) {
          setSearches((prev) => (prev.length === 0 && list.length === 0 ? prev : list));
        }
      } catch {
        // Ignore platform errors during initial fetch
      }
    })();
    return () => {
      active = false;
    };
  }, [platform]);

  const handleCreate = useCallback(async () => {
    const trimmedName = name.trim();
    const trimmedQuery = query.trim();
    if (!trimmedName || !trimmedQuery) return;

    const newSearch: SavedSearch = {
      id: `search-${Date.now()}`,
      name: trimmedName,
      query: trimmedQuery,
      notifyOnMatch,
      createdAt: new Date().toISOString(),
    };

    const created = await platform.createSavedSearch?.(newSearch);
    const list = await platform.listSavedSearches?.();
    if (list && list.length > 0) {
      setSearches(list);
    } else if (created) {
      setSearches((prev) => [...prev, created]);
    }

    setName('');
    setQuery('');
  }, [name, query, notifyOnMatch, platform]);

  const handleDelete = useCallback(
    async (id: string) => {
      await platform.deleteSavedSearch?.(id);
      const list = await platform.listSavedSearches?.();
      if (list && list.length > 0) {
        setSearches(list.filter((s) => s.id !== id));
      } else {
        setSearches((prev) => prev.filter((s) => s.id !== id));
      }
      setMatchesBySearch((prev) => {
        if (!(id in prev)) return prev;
        const next = { ...prev };
        delete next[id];
        return next;
      });
    },
    [platform],
  );

  const loadMatches = useCallback(
    async (id: string) => {
      const matches = (await platform.savedSearchMatches?.(id)) ?? [];
      setMatchesBySearch((prev) => ({
        ...prev,
        [id]: matches,
      }));
    },
    [platform],
  );

  return (
    <div className={styles.tabSection}>
      <div className={styles.createCard}>
        <input
          type="text"
          className={styles.textInput}
          aria-label="Saved search name"
          placeholder="Name (e.g. Tankers)"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <input
          type="text"
          className={styles.textInput}
          aria-label="Saved search query"
          placeholder="Query (e.g. B744, US Navy)"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <div className={styles.formRow}>
          <div className={styles.toggleLabel}>
            <Toggle
              aria-label="Alert on match"
              checked={notifyOnMatch}
              onCheckedChange={(checked) => setNotifyOnMatch(checked)}
            />
            <span>Alert on match</span>
          </div>
          <Button
            size="sm"
            disabled={!name.trim() || !query.trim()}
            onClick={() => void handleCreate()}
          >
            Save Search
          </Button>
        </div>
      </div>

      <ol className={styles.list} aria-label="Saved searches">
        {searches.map((search) => {
          const matches = matchesBySearch[search.id];
          return (
            <li key={search.id} className={styles.item}>
              <div className={styles.itemHeader}>
                <div className={styles.badges}>
                  <span className={styles.entityBadge}>{search.query}</span>
                  {search.notifyOnMatch ? (
                    <span className={styles.kindSearchMatch}>ALERT ON</span>
                  ) : null}
                </div>
              </div>

              <div className={styles.itemBody}>
                <h3 className={styles.entityLabel}>{search.name}</h3>
              </div>

              <div className={styles.actions}>
                <Button size="sm" variant="secondary" onClick={() => void loadMatches(search.id)}>
                  Matches
                </Button>
                <Button size="sm" variant="ghost" onClick={() => void handleDelete(search.id)}>
                  Delete
                </Button>
              </div>

              {matches ? (
                matches.length === 0 ? (
                  <p className={styles.details}>No matches recorded.</p>
                ) : (
                  <ul className={styles.matchesList} aria-label={`Matches for ${search.name}`}>
                    {matches.map((match) => (
                      <li
                        key={`${match.feed}-${match.entityId}-${match.matchedAt}`}
                        className={styles.matchRow}
                      >
                        <div className={styles.matchMeta}>
                          <span className={styles.entityBadge}>{match.feed}</span>
                          <span className={styles.matchLabel}>{match.label || match.entityId}</span>
                        </div>
                        <Button
                          size="sm"
                          variant="secondary"
                          onClick={() => select({ feed: match.feed, id: match.entityId })}
                        >
                          Focus Match
                        </Button>
                      </li>
                    ))}
                  </ul>
                )
              ) : null}
            </li>
          );
        })}
      </ol>
    </div>
  );
}
