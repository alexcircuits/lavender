import { CommandInput, CommandRoot } from 'cmdk';
import { type KeyboardEvent, useCallback, useRef, useState } from 'react';
import { cx, IconButton, Kbd, MagnifyingGlassIcon, XIcon } from '../../design';
import { SEARCH_INPUT_ATTRIBUTE, useCommandContext } from '../commands';
import { SearchResults } from './SearchResults';
import styles from './search.module.css';
import { useSearchModel } from './useSearch';

export interface SearchBarProps {
  readonly className?: string | undefined;
}

function InlineResults({ query, onDone }: { query: string; onDone: () => void }) {
  const commandContext = useCommandContext();
  const model = useSearchModel(query, { commands: false, onDone, commandContext });
  return <SearchResults query={query} model={model} keepFocus />;
}

/**
 * The search field at top centre (design-system §6, §8): `/` focuses it, results appear inline
 * as you type, Enter runs the highlighted one. ⌘K opens the full palette instead.
 */
export function SearchBar({ className }: SearchBarProps) {
  const [query, setQuery] = useState('');
  const [focused, setFocused] = useState(false);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const open = focused && query.trim() !== '';
  const clear = useCallback(() => setQuery(''), []);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== 'Escape') return;
    if (query !== '') {
      event.preventDefault();
      setQuery('');
    } else {
      inputRef.current?.blur();
    }
  };

  return (
    <CommandRoot
      label="Search places, coordinates and callsigns"
      shouldFilter={false}
      vimBindings={false}
      className={cx(styles.bar, className)}
      onKeyDown={onKeyDown}
    >
      <div className={styles.field}>
        <MagnifyingGlassIcon size={16} weight="bold" aria-hidden className={styles.fieldIcon} />
        <CommandInput
          ref={inputRef}
          {...{ [SEARCH_INPUT_ATTRIBUTE]: '' }}
          value={query}
          onValueChange={setQuery}
          onFocus={() => setFocused(true)}
          onBlur={() => setFocused(false)}
          placeholder="Search places, coordinates, callsigns"
          className={styles.input}
        />
        {query !== '' ? (
          <IconButton
            label="Clear search"
            icon={XIcon}
            size="sm"
            onClick={() => {
              setQuery('');
              inputRef.current?.focus();
            }}
          />
        ) : (
          <Kbd aria-hidden>/</Kbd>
        )}
      </div>
      {open ? (
        <div className={styles.dropdown}>
          <InlineResults query={query} onDone={clear} />
        </div>
      ) : null}
    </CommandRoot>
  );
}
