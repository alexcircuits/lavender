import { CommandGroup, CommandItem, CommandList } from 'cmdk';
import type { MouseEvent } from 'react';
import { cx, Kbd } from '../../design';
import type { ResultItem } from './results';
import styles from './search.module.css';
import type { SearchModel } from './useSearch';

export interface SearchResultsProps {
  readonly query: string;
  readonly model: SearchModel;
  readonly className?: string | undefined;
  /** Inline results keep focus in the field: pressing in the list must not blur it. */
  readonly keepFocus?: boolean;
}

const preventBlur = (event: MouseEvent<HTMLDivElement>) => event.preventDefault();

function Item({ item }: { item: ResultItem }) {
  const Glyph = item.icon;
  return (
    <CommandItem
      value={item.id}
      disabled={item.disabled ?? false}
      onSelect={() => item.run()}
      className={styles.item}
    >
      <Glyph size={16} weight="bold" aria-hidden className={styles.itemIcon} />
      <span className={styles.itemText}>
        <span className={styles.itemTitle}>{item.title}</span>
        {item.detail ? <span className={styles.itemDetail}>{item.detail}</span> : null}
      </span>
      {item.shortcut ? <Kbd aria-hidden>{item.shortcut}</Kbd> : null}
      <span className={styles.itemHint} aria-hidden>
        {item.hint}
      </span>
    </CommandItem>
  );
}

/** What the person should know about the result set: searching, nothing found, degraded. */
function StatusLine({ query, model }: { query: string; model: SearchModel }) {
  const q = query.trim();
  let text: string | null = null;
  if (model.status === 'failed') {
    text = 'Place search is unavailable. Coordinates and live entities still work.';
  } else if (model.empty) {
    text = `No matches for '${q}'. Try a place name, coordinates or a callsign.`;
  } else if (model.status === 'loading' && q !== '') {
    text = 'Searching places…';
  }
  return (
    <div role="status" aria-live="polite" className={styles.statusLines}>
      {text ? <p className={styles.statusLine}>{text}</p> : null}
      {model.notices.map((notice) => (
        <p key={notice} className={styles.notice}>
          {notice}
        </p>
      ))}
    </div>
  );
}

/**
 * The grouped result list shared by the top search field and the palette. Must render inside a
 * cmdk `CommandRoot`, which owns arrow keys, Home/End and Enter.
 */
export function SearchResults({ query, model, className, keepFocus = false }: SearchResultsProps) {
  return (
    <>
      <CommandList
        label="Results"
        className={cx(styles.list, className)}
        {...(keepFocus ? { onMouseDown: preventBlur } : {})}
      >
        {model.groups.map((group) => (
          <CommandGroup key={group.id} value={group.id} heading={group.heading}>
            {group.items.map((item) => (
              <Item key={item.id} item={item} />
            ))}
            {group.footer ? <p className={styles.groupFooter}>{group.footer}</p> : null}
          </CommandGroup>
        ))}
      </CommandList>
      <StatusLine query={query} model={model} />
    </>
  );
}
