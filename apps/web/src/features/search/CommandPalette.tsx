import { CommandInput, CommandRoot } from 'cmdk';
import { useCallback, useState } from 'react';
import { Dialog, Kbd, MagnifyingGlassIcon } from '../../design';
import { useUi } from '../../runtime';
import { useCommandContext } from '../commands';
import { SearchResults } from './SearchResults';
import styles from './search.module.css';
import { useSearchModel } from './useSearch';

function PaletteBody({ onDone }: { onDone: () => void }) {
  const [query, setQuery] = useState('');
  const commandContext = useCommandContext();
  const model = useSearchModel(query, { commands: true, onDone, commandContext });
  return (
    <CommandRoot
      label="Search places, coordinates, callsigns and commands"
      shouldFilter={false}
      vimBindings={false}
      loop
      className={styles.command}
    >
      <div className={styles.paletteField}>
        <MagnifyingGlassIcon size={16} weight="bold" aria-hidden className={styles.fieldIcon} />
        <CommandInput
          value={query}
          onValueChange={setQuery}
          placeholder="Search places, coordinates, callsigns or commands"
          className={styles.input}
        />
      </div>
      <SearchResults query={query} model={model} className={styles.paletteList} />
      <footer className={styles.legend} aria-hidden>
        <span>
          <Kbd>↑</Kbd> <Kbd>↓</Kbd> to move
        </span>
        <span>
          <Kbd>Enter</Kbd> to choose
        </span>
        <span>
          <Kbd>Esc</Kbd> to close
        </span>
      </footer>
    </CommandRoot>
  );
}

/**
 * The command palette (design-system §7, §8): ⌘K / Ctrl K. Places, live entities, layers and
 * actions in one keyboard-first list, in the Dialog primitive so focus is trapped and returns
 * to wherever it was when the palette opened.
 */
export function CommandPalette() {
  const open = useUi((s) => s.paletteOpen);
  const setOpen = useUi((s) => s.setPaletteOpen);
  const close = useCallback(() => setOpen(false), [setOpen]);
  return (
    <Dialog
      open={open}
      onOpenChange={setOpen}
      title="Search and commands"
      className={styles.palette}
    >
      {open ? <PaletteBody onDone={close} /> : null}
    </Dialog>
  );
}
