import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { Toggle } from './Toggle';

function Controlled({ onChange }: { onChange: (checked: boolean) => void }) {
  const [on, setOn] = useState(false);
  return (
    <Toggle
      aria-label="Aircraft"
      checked={on}
      onCheckedChange={(next) => {
        setOn(next);
        onChange(next);
      }}
    />
  );
}

describe('Toggle', () => {
  it('is a switch that announces on/off through aria-checked', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<Controlled onChange={onChange} />);
    const toggle = screen.getByRole('switch', { name: 'Aircraft' });
    expect(toggle).toHaveAttribute('aria-checked', 'false');
    await user.click(toggle);
    expect(toggle).toHaveAttribute('aria-checked', 'true');
    expect(toggle).toHaveAttribute('data-checked');
    expect(onChange).toHaveBeenLastCalledWith(true);
  });

  it('toggles with Space and Enter when focused', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<Controlled onChange={onChange} />);
    await user.tab();
    const toggle = screen.getByRole('switch', { name: 'Aircraft' });
    expect(toggle).toHaveFocus();
    await user.keyboard(' ');
    expect(toggle).toHaveAttribute('aria-checked', 'true');
    await user.keyboard('{Enter}');
    expect(toggle).toHaveAttribute('aria-checked', 'false');
    expect(onChange).toHaveBeenCalledTimes(2);
  });

  it('ignores input while disabled', async () => {
    const user = userEvent.setup();
    const onCheckedChange = vi.fn();
    render(<Toggle aria-label="Fires" disabled onCheckedChange={onCheckedChange} />);
    const toggle = screen.getByRole('switch', { name: 'Fires' });
    expect(toggle).toHaveAttribute('aria-disabled', 'true');
    await user.click(toggle);
    expect(onCheckedChange).not.toHaveBeenCalled();
  });
});
