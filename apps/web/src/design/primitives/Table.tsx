import { useVirtualizer } from '@tanstack/react-virtual';
import { type ReactNode, useRef } from 'react';
import { cx } from '../cx';
import { ArrowDownIcon, ArrowUpIcon } from '../icons';
import styles from './Table.module.css';

export type SortDirection = 'ascending' | 'descending';

export interface TableSort {
  columnId: string;
  direction: SortDirection;
}

export interface TableColumn<Row> {
  id: string;
  /** Sentence case header. */
  header: string;
  cell: (row: Row) => ReactNode;
  /** `end` for numbers: right-aligned, tabular. */
  align?: 'start' | 'end';
  /** Mono face for identifiers (ICAO hex, MMSI, event IDs). */
  mono?: boolean;
  /** CSS width of the column, e.g. "96px" or "30%". */
  width?: string;
  sortable?: boolean;
}

export interface TableProps<Row> {
  'aria-label': string;
  columns: ReadonlyArray<TableColumn<Row>>;
  rows: ReadonlyArray<Row>;
  getRowId: (row: Row) => string;
  sort?: TableSort;
  onSortChange?: (sort: TableSort) => void;
  /** Height of the scrolling region (CSS length). The header stays visible while scrolling. */
  maxHeight?: string;
  /** Minimum table width (CSS length); narrower containers scroll horizontally instead of
   * crushing columns. */
  minWidth?: string;
  /** Row height in px; the spec's 32. */
  rowHeight?: number;
  /** Rows are virtualised past this count (spec: 200). */
  virtualizeAbove?: number;
  className?: string | undefined;
}

export const VIRTUALIZE_ABOVE = 200;
const OVERSCAN = 8;

/**
 * Data table: sticky `--type-micro` header, 32 px rows, hairlines between rows, numbers right
 * aligned and tabular, mono IDs, `aria-sort` on sortable columns, and row virtualisation past
 * 200 rows (TanStack Virtual) with aria-rowcount/aria-rowindex so the full size is announced.
 */
export function Table<Row>({
  columns,
  rows,
  getRowId,
  sort,
  onSortChange,
  maxHeight = '360px',
  minWidth,
  rowHeight = 32,
  virtualizeAbove = VIRTUALIZE_ABOVE,
  className,
  'aria-label': ariaLabel,
}: TableProps<Row>) {
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const virtualize = rows.length > virtualizeAbove;
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    overscan: OVERSCAN,
    enabled: virtualize,
  });

  const items = virtualize
    ? virtualizer.getVirtualItems().map((item) => ({ index: item.index, start: item.start }))
    : rows.map((_, index) => ({ index, start: index * rowHeight }));
  const first = items[0];
  const last = items[items.length - 1];
  const padTop = virtualize && first ? first.start : 0;
  const padBottom =
    virtualize && last ? Math.max(0, virtualizer.getTotalSize() - (last.start + rowHeight)) : 0;

  return (
    <div
      ref={scrollRef}
      className={cx(styles.scroller, className)}
      style={{ maxHeight }}
      data-virtualized={virtualize ? '' : undefined}
    >
      <table
        className={styles.table}
        aria-label={ariaLabel}
        aria-rowcount={rows.length + 1}
        style={minWidth ? { minWidth } : undefined}
      >
        <colgroup>
          {columns.map((column) => (
            <col key={column.id} style={column.width ? { width: column.width } : undefined} />
          ))}
        </colgroup>
        <thead className={styles.head}>
          <tr aria-rowindex={1}>
            {columns.map((column) => (
              <HeaderCell
                key={column.id}
                column={column}
                sort={sort}
                {...(onSortChange ? { onSortChange } : {})}
              />
            ))}
          </tr>
        </thead>
        <tbody>
          {padTop > 0 ? (
            // biome-ignore lint/a11y/noAriaHiddenOnFocusable: a spacer <tr> has no tabindex and no content; Biome treats every <tr> as focusable.
            <tr aria-hidden="true" className={styles.spacer} style={{ height: padTop }} />
          ) : null}
          {items.map(({ index }) => {
            const row = rows[index];
            if (row === undefined) return null;
            return (
              <tr
                key={getRowId(row)}
                aria-rowindex={index + 2}
                className={styles.row}
                style={{ height: rowHeight }}
              >
                {columns.map((column) => (
                  <td
                    key={column.id}
                    data-align={column.align ?? 'start'}
                    className={cx(styles.cell, column.mono && 'mono')}
                  >
                    {column.cell(row)}
                  </td>
                ))}
              </tr>
            );
          })}
          {padBottom > 0 ? (
            // biome-ignore lint/a11y/noAriaHiddenOnFocusable: a spacer <tr> has no tabindex and no content; Biome treats every <tr> as focusable.
            <tr aria-hidden="true" className={styles.spacer} style={{ height: padBottom }} />
          ) : null}
        </tbody>
      </table>
    </div>
  );
}

interface HeaderCellProps<Row> {
  column: TableColumn<Row>;
  sort: TableSort | undefined;
  onSortChange?: (sort: TableSort) => void;
}

function HeaderCell<Row>({ column, sort, onSortChange }: HeaderCellProps<Row>) {
  const align = column.align ?? 'start';
  const active = sort?.columnId === column.id ? sort.direction : undefined;
  if (!column.sortable || !onSortChange) {
    return (
      <th scope="col" data-align={align} className={styles.header}>
        {column.header}
      </th>
    );
  }
  const SortGlyph = active === 'descending' ? ArrowDownIcon : ArrowUpIcon;
  const next: SortDirection = active === 'ascending' ? 'descending' : 'ascending';
  return (
    <th scope="col" data-align={align} aria-sort={active ?? 'none'} className={styles.header}>
      <button
        type="button"
        className={styles.sortButton}
        data-active={active ? '' : undefined}
        onClick={() => onSortChange({ columnId: column.id, direction: next })}
      >
        <span>{column.header}</span>
        <SortGlyph size={12} weight="bold" aria-hidden className={styles.sortGlyph} />
      </button>
    </th>
  );
}
