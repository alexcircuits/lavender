import {
  type ChangeEvent,
  type ComponentPropsWithRef,
  type KeyboardEvent,
  useRef,
  useState,
} from 'react';
import { MagnifyingGlassIcon, XIcon } from '../icons';
import { IconButton } from './IconButton';
import { Kbd } from './Kbd';
import styles from './SearchField.module.css';
import { TextField } from './TextField';

export interface SearchFieldProps
  extends Omit<
    ComponentPropsWithRef<'input'>,
    'type' | 'size' | 'value' | 'defaultValue' | 'onChange'
  > {
  /** Accessible name; visually hidden because the glyph and placeholder carry it. */
  label: string;
  value?: string;
  defaultValue?: string;
  onValueChange?: (value: string) => void;
  /** Key hint shown while the field is empty, e.g. "/" or "⌘K". */
  shortcutHint?: string;
  size?: 'sm' | 'md';
}

/**
 * Search input: leading magnifier, a clear button once there is a query, and a key hint while
 * empty. `Esc` clears a query; on an empty field it blurs, handing focus back to the globe.
 */
export function SearchField({
  label,
  value,
  defaultValue = '',
  onValueChange,
  shortcutHint,
  size = 'md',
  onKeyDown,
  ref,
  ...rest
}: SearchFieldProps) {
  const [uncontrolled, setUncontrolled] = useState(defaultValue);
  const query = value ?? uncontrolled;
  const inputRef = useRef<HTMLInputElement | null>(null);

  const setQuery = (next: string) => {
    if (value === undefined) setUncontrolled(next);
    onValueChange?.(next);
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    onKeyDown?.(event);
    if (event.defaultPrevented || event.key !== 'Escape') return;
    if (query !== '') {
      event.preventDefault();
      setQuery('');
    } else {
      event.currentTarget.blur();
    }
  };

  const setRefs = (node: HTMLInputElement | null) => {
    inputRef.current = node;
    if (typeof ref === 'function') ref(node);
    else if (ref) ref.current = node;
  };

  const trailing =
    query !== '' ? (
      <IconButton
        label="Clear search"
        icon={XIcon}
        size="sm"
        className={styles.clear}
        onClick={() => {
          setQuery('');
          inputRef.current?.focus();
        }}
      />
    ) : shortcutHint ? (
      <Kbd aria-hidden>{shortcutHint}</Kbd>
    ) : null;

  return (
    <TextField
      {...rest}
      ref={setRefs}
      type="search"
      label={label}
      hideLabel
      size={size}
      value={query}
      onChange={(event: ChangeEvent<HTMLInputElement>) => setQuery(event.target.value)}
      onKeyDown={handleKeyDown}
      autoComplete="off"
      spellCheck={false}
      leading={<MagnifyingGlassIcon size={16} weight="bold" aria-hidden />}
      trailing={trailing}
      className={styles.search}
    />
  );
}
