import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Table, type TableColumn } from './Table';

interface Quake {
  id: string;
  magnitude: number;
  place: string;
}

const COLUMNS: ReadonlyArray<TableColumn<Quake>> = [
  { id: 'id', header: 'Event ID', cell: (q) => q.id, mono: true },
  { id: 'place', header: 'Place', cell: (q) => q.place },
  {
    id: 'magnitude',
    header: 'Magnitude',
    cell: (q) => q.magnitude.toFixed(1),
    align: 'end',
    sortable: true,
  },
];

function quakes(count: number): Quake[] {
  return Array.from({ length: count }, (_, i) => ({
    id: `us7000${String(i).padStart(4, '0')}`,
    magnitude: 2.5 + (i % 50) / 10,
    place: `Event ${i + 1}`,
  }));
}

/** happy-dom has no layout: give the scroll container a real-looking 320 × 800 viewport. */
function stubLayout() {
  const height = vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(320);
  const width = vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(800);
  return () => {
    height.mockRestore();
    width.mockRestore();
  };
}

describe('Table', () => {
  let restore: () => void = () => undefined;
  beforeEach(() => {
    restore = stubLayout();
  });
  afterEach(() => restore());

  it('renders every row below the virtualisation threshold, with a named table and headers', () => {
    render(
      <Table aria-label="Earthquakes" columns={COLUMNS} rows={quakes(50)} getRowId={(q) => q.id} />,
    );
    const table = screen.getByRole('table', { name: 'Earthquakes' });
    expect(
      within(table)
        .getAllByRole('columnheader')
        .map((h) => h.textContent),
    ).toEqual(['Event ID', 'Place', 'Magnitude']);
    // 50 body rows + 1 header row.
    expect(within(table).getAllByRole('row')).toHaveLength(51);
    expect(table).toHaveAttribute('aria-rowcount', '51');
  });

  it('virtualises 1 000 rows: the DOM holds a bounded window, the full size is announced', () => {
    render(
      <Table
        aria-label="Earthquakes"
        columns={COLUMNS}
        rows={quakes(1000)}
        getRowId={(q) => q.id}
      />,
    );
    const table = screen.getByRole('table', { name: 'Earthquakes' });
    const bodyRows = table.querySelectorAll('tbody tr[aria-rowindex]');
    expect(bodyRows.length).toBeGreaterThan(5);
    expect(bodyRows.length).toBeLessThan(40);
    expect(table).toHaveAttribute('aria-rowcount', '1001');
    expect(bodyRows[0]).toHaveAttribute('aria-rowindex', '2');
    expect(screen.getByText('us70000000')).toBeInTheDocument();
    expect(screen.queryByText('us70000999')).not.toBeInTheDocument();
  });

  it('right-aligns numeric columns and uses mono for identifiers', () => {
    render(
      <Table aria-label="Earthquakes" columns={COLUMNS} rows={quakes(3)} getRowId={(q) => q.id} />,
    );
    const cells = screen.getAllByRole('cell');
    expect(cells[0]).toHaveClass('mono');
    expect(cells[2]).toHaveAttribute('data-align', 'end');
  });

  it('exposes sort state with aria-sort and toggles direction from the header button', async () => {
    const user = userEvent.setup();
    const onSortChange = vi.fn();
    const { rerender } = render(
      <Table
        aria-label="Earthquakes"
        columns={COLUMNS}
        rows={quakes(3)}
        getRowId={(q) => q.id}
        onSortChange={onSortChange}
      />,
    );
    const header = screen.getByRole('columnheader', { name: 'Magnitude' });
    expect(header).toHaveAttribute('aria-sort', 'none');
    // Only sortable columns announce a sort state.
    expect(screen.getByRole('columnheader', { name: 'Place' })).not.toHaveAttribute('aria-sort');
    await user.click(within(header).getByRole('button', { name: 'Magnitude' }));
    expect(onSortChange).toHaveBeenLastCalledWith({
      columnId: 'magnitude',
      direction: 'ascending',
    });

    rerender(
      <Table
        aria-label="Earthquakes"
        columns={COLUMNS}
        rows={quakes(3)}
        getRowId={(q) => q.id}
        sort={{ columnId: 'magnitude', direction: 'ascending' }}
        onSortChange={onSortChange}
      />,
    );
    expect(screen.getByRole('columnheader', { name: 'Magnitude' })).toHaveAttribute(
      'aria-sort',
      'ascending',
    );
    await user.click(screen.getByRole('button', { name: 'Magnitude' }));
    expect(onSortChange).toHaveBeenLastCalledWith({
      columnId: 'magnitude',
      direction: 'descending',
    });
  });
});
