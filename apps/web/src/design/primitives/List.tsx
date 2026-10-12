import {
  type ComponentPropsWithoutRef,
  type ComponentPropsWithRef,
  createContext,
  type KeyboardEvent,
  type MouseEvent,
  type ReactNode,
  useContext,
  useLayoutEffect,
  useRef,
} from 'react';
import { cx } from '../cx';
import styles from './List.module.css';

const ROW_SELECTOR = '[data-list-row]:not([data-disabled])';
const NEXT_KEYS = new Set(['ArrowDown', 'j']);
const PREVIOUS_KEYS = new Set(['ArrowUp', 'k']);

interface ListContextValue {
  selectable: boolean;
  dense: boolean;
}

const ListContext = createContext<ListContextValue>({ selectable: false, dense: false });

function isTyping(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName);
}

function rowsOf(list: HTMLElement): HTMLElement[] {
  return [...list.querySelectorAll<HTMLElement>(ROW_SELECTOR)];
}

/** Exactly one row is in the Tab order: the focused one, else the selected one, else the first. */
function syncTabStops(list: HTMLElement): void {
  const rows = rowsOf(list);
  const active = document.activeElement;
  const current =
    rows.find((row) => row === active || row.contains(active)) ??
    rows.find((row) => row.hasAttribute('data-selected')) ??
    rows[0];
  for (const row of list.querySelectorAll<HTMLElement>('[data-list-row]')) {
    row.tabIndex = row === current ? 0 : -1;
  }
}

export interface ListProps extends Omit<ComponentPropsWithoutRef<'ul'>, 'role'> {
  'aria-label': string;
  /** Rows are options that can be selected (listbox); otherwise a plain list of rows. */
  selectable?: boolean;
  dense?: boolean;
  children: ReactNode;
}

/**
 * Keyboard-navigable list: `j`/`k` or arrow keys move between rows, Home/End jump; ignored while
 * typing. Selection is the caller's: rows call `onSelect` on click, Enter or Space.
 */
export function List({
  selectable = false,
  dense = false,
  className,
  children,
  onKeyDown,
  onFocus,
  ...rest
}: ListProps) {
  const ref = useRef<HTMLUListElement | null>(null);

  useLayoutEffect(() => {
    if (ref.current) syncTabStops(ref.current);
  });

  const handleKeyDown = (event: KeyboardEvent<HTMLUListElement>) => {
    onKeyDown?.(event);
    const list = ref.current;
    if (!list || event.defaultPrevented || isTyping(event.target)) return;
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    const rows = rowsOf(list);
    if (rows.length === 0) return;
    const index = rows.findIndex((row) => row.contains(event.target as Node));
    // Keys pressed in non-row content inside the list (an options panel) are not ours.
    if (index < 0 && event.target !== list) return;
    let next: HTMLElement | undefined;
    if (NEXT_KEYS.has(event.key)) next = rows[Math.min(rows.length - 1, index + 1)];
    else if (PREVIOUS_KEYS.has(event.key)) next = rows[Math.max(0, index - 1)];
    else if (event.key === 'Home') next = rows[0];
    else if (event.key === 'End') next = rows[rows.length - 1];
    if (!next) return;
    event.preventDefault();
    next.focus();
    syncTabStops(list);
  };

  return (
    <ListContext.Provider value={{ selectable, dense }}>
      <ul
        {...rest}
        ref={ref}
        role={selectable ? 'listbox' : 'list'}
        className={cx(styles.list, className)}
        onKeyDown={handleKeyDown}
        onFocus={(event) => {
          onFocus?.(event);
          if (ref.current) syncTabStops(ref.current);
        }}
      >
        {children}
      </ul>
    </ListContext.Provider>
  );
}

export interface ListRowProps
  extends Omit<ComponentPropsWithRef<'li'>, 'title' | 'onSelect' | 'role'> {
  title: ReactNode;
  /** Glyph or swatch before the title. */
  leading?: ReactNode;
  /** Second line in `--ink-2`. */
  description?: ReactNode;
  /** Trailing meta (counts, ages) in `--ink-2`, tabular. */
  meta?: ReactNode;
  /** Trailing controls (toggle, disclosure). Clicks on them never select the row. */
  trailing?: ReactNode;
  selected?: boolean;
  disabled?: boolean;
  onSelect?: () => void;
}

/** A 32 px row (28 dense): hover `--island-hover`, selected `--accent-subtle` + `--ink`. */
export function ListRow({
  title,
  leading,
  description,
  meta,
  trailing,
  selected = false,
  disabled = false,
  onSelect,
  className,
  onClick,
  onKeyDown,
  ...rest
}: ListRowProps) {
  const { selectable, dense } = useContext(ListContext);

  const handleClick = (event: MouseEvent<HTMLLIElement>) => {
    onClick?.(event);
    if (disabled || event.defaultPrevented) return;
    if ((event.target as HTMLElement).closest('[data-row-control]')) return;
    onSelect?.();
  };
  const handleKeyDown = (event: KeyboardEvent<HTMLLIElement>) => {
    onKeyDown?.(event);
    if (disabled || event.defaultPrevented || event.target !== event.currentTarget) return;
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      onSelect?.();
    }
  };

  // In a listbox each row is an option with aria-selected; otherwise a plain list item, where
  // aria-selected is not allowed.
  const roleProps = selectable
    ? ({ role: 'option', 'aria-selected': selected } as const)
    : ({ role: 'listitem' } as const);

  return (
    <li
      {...rest}
      {...roleProps}
      aria-disabled={disabled || undefined}
      tabIndex={-1}
      data-list-row=""
      data-selected={selected ? '' : undefined}
      data-disabled={disabled ? '' : undefined}
      data-dense={dense ? '' : undefined}
      data-interactive={onSelect ? '' : undefined}
      className={cx(styles.row, className)}
      onClick={handleClick}
      onKeyDown={handleKeyDown}
    >
      {leading ? <span className={styles.leading}>{leading}</span> : null}
      <span className={styles.main}>
        <span className={styles.title}>{title}</span>
        {description ? <span className={styles.description}>{description}</span> : null}
      </span>
      {meta ? <span className={styles.meta}>{meta}</span> : null}
      {trailing ? (
        <span className={styles.trailing} data-row-control="">
          {trailing}
        </span>
      ) : null}
    </li>
  );
}
