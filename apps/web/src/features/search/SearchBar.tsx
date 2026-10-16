import { CommandInput, CommandRoot } from 'cmdk';
import { type KeyboardEvent, useCallback, useRef, useState } from 'react';
import { cx, IconButton, Kbd, MagnifyingGlassIcon, XIcon } from '../../design';
import { SEARCH_INPUT_ATTRIBUTE, useCommandContext } from '../commands';
import { SearchResults } from './SearchResults';
import styles from './search.module.css';
import { useResultSelection } from './useResultSelection';
import { useSearchModel } from './useSearch';

export interface SearchBarProps {
  readonly className?: string | undefined;
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
  const commandContext = useCommandContext();
  const model = useSearchModel(open ? query : '', {
    commands: false,
    onDone: clear,
    commandContext,
  });
  const selection = useResultSelection(query, model.groups);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    selection.onKeyDown(event);
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
      value={selection.value}
      onValueChange={selection.onValueChange}
      onPointerMoveCapture={selection.onPointerMoveCapture}
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
          placeholder="Search places, coordinates, callsigns, ships"
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
          <SearchResults query={query} model={model} keepFocus />
        </div>
      ) : null}
    </CommandRoot>
  );
}
