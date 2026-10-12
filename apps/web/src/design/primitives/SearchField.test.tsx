import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { SearchField } from './SearchField';

describe('SearchField', () => {
  it('is a labelled search box with a key hint while empty', () => {
    render(<SearchField label="Search places, coordinates and callsigns" shortcutHint="/" />);
    expect(
      screen.getByRole('searchbox', { name: 'Search places, coordinates and callsigns' }),
    ).toBeInTheDocument();
    expect(screen.getByText('/').tagName).toBe('KBD');
    expect(screen.queryByRole('button', { name: 'Clear search' })).not.toBeInTheDocument();
  });

  it('shows a clear button once there is a query, and clearing refocuses the field', async () => {
    const user = userEvent.setup();
    const onValueChange = vi.fn();
    render(<SearchField label="Search" shortcutHint="/" onValueChange={onValueChange} />);
    const box = screen.getByRole('searchbox', { name: 'Search' });
    await user.type(box, 'Helsinki');
    expect(onValueChange).toHaveBeenLastCalledWith('Helsinki');
    await user.click(screen.getByRole('button', { name: 'Clear search' }));
    expect(box).toHaveValue('');
    expect(box).toHaveFocus();
  });

  it('clears on Escape, then blurs on a second Escape', async () => {
    const user = userEvent.setup();
    render(<SearchField label="Search" />);
    const box = screen.getByRole('searchbox', { name: 'Search' });
    await user.type(box, '60.1699, 24.9384');
    await user.keyboard('{Escape}');
    expect(box).toHaveValue('');
    expect(box).toHaveFocus();
    await user.keyboard('{Escape}');
    expect(box).not.toHaveFocus();
  });
});
