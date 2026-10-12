import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { List, ListRow } from './List';
import { Toggle } from './Toggle';

const CONTACTS = ['SAS1462', 'FIN7LY', 'NAX5KM', 'DLH2TA'];

function Contacts({ onSelect }: { onSelect: (id: string) => void }) {
  const [selected, setSelected] = useState<string | null>(null);
  return (
    <List aria-label="Contacts" selectable>
      {CONTACTS.map((id) => (
        <ListRow
          key={id}
          title={id}
          selected={selected === id}
          onSelect={() => {
            setSelected(id);
            onSelect(id);
          }}
        />
      ))}
    </List>
  );
}

describe('List', () => {
  it('is a listbox of options with a single tab stop', () => {
    render(<Contacts onSelect={vi.fn()} />);
    expect(screen.getByRole('listbox', { name: 'Contacts' })).toBeInTheDocument();
    const options = screen.getAllByRole('option');
    expect(options).toHaveLength(4);
    expect(options.filter((o) => o.tabIndex === 0)).toHaveLength(1);
  });

  it('moves with j/k and arrows, and selects with Enter', async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(<Contacts onSelect={onSelect} />);
    await user.tab();
    expect(screen.getByRole('option', { name: 'SAS1462' })).toHaveFocus();
    await user.keyboard('j');
    expect(screen.getByRole('option', { name: 'FIN7LY' })).toHaveFocus();
    await user.keyboard('{ArrowDown}');
    expect(screen.getByRole('option', { name: 'NAX5KM' })).toHaveFocus();
    await user.keyboard('k');
    expect(screen.getByRole('option', { name: 'FIN7LY' })).toHaveFocus();
    await user.keyboard('{End}');
    expect(screen.getByRole('option', { name: 'DLH2TA' })).toHaveFocus();
    await user.keyboard('{Enter}');
    expect(onSelect).toHaveBeenLastCalledWith('DLH2TA');
    expect(screen.getByRole('option', { name: 'DLH2TA' })).toHaveAttribute('aria-selected', 'true');
  });

  it('does not select the row when a trailing control is used', async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(
      <List aria-label="Layers">
        <ListRow title="Aircraft" onSelect={onSelect} trailing={<Toggle aria-label="Aircraft" />} />
      </List>,
    );
    expect(screen.getByRole('list', { name: 'Layers' })).toBeInTheDocument();
    await user.click(screen.getByRole('switch', { name: 'Aircraft' }));
    expect(onSelect).not.toHaveBeenCalled();
    expect(screen.getByRole('listitem')).not.toHaveAttribute('aria-selected');
  });
});
