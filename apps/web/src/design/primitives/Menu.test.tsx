import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { Button } from './Button';
import {
  Menu,
  MenuContent,
  MenuItem,
  MenuRadioGroup,
  MenuRadioItem,
  MenuSeparator,
  MenuTrigger,
} from './Menu';

function ImageryMenu({ onCopy }: { onCopy: () => void }) {
  const [imagery, setImagery] = useState('satellite');
  return (
    <Menu>
      <MenuTrigger render={<Button>Imagery</Button>} />
      <MenuContent>
        <MenuRadioGroup value={imagery} onValueChange={(value: string) => setImagery(value)}>
          <MenuRadioItem value="satellite">Satellite</MenuRadioItem>
          <MenuRadioItem value="streets">Dark streets</MenuRadioItem>
          <MenuRadioItem value="terrain">Terrain</MenuRadioItem>
        </MenuRadioGroup>
        <MenuSeparator />
        <MenuItem onClick={onCopy} shortcut="⌘C">
          Copy view link
        </MenuItem>
      </MenuContent>
    </Menu>
  );
}

describe('Menu', () => {
  it('opens from the keyboard, moves with arrows, activates with Enter and returns focus', async () => {
    const user = userEvent.setup();
    const onCopy = vi.fn();
    render(<ImageryMenu onCopy={onCopy} />);
    const trigger = screen.getByRole('button', { name: 'Imagery' });
    expect(trigger).toHaveAttribute('aria-haspopup', 'menu');

    await user.tab();
    expect(trigger).toHaveFocus();
    await user.keyboard('{Enter}');
    const menu = await screen.findByRole('menu');
    expect(menu).toBeVisible();
    expect(trigger).toHaveAttribute('aria-expanded', 'true');

    await user.keyboard('{End}');
    expect(screen.getByRole('menuitem', { name: /Copy view link/ })).toHaveFocus();
    await user.keyboard('{Enter}');
    expect(onCopy).toHaveBeenCalledOnce();
    await waitFor(() => expect(screen.queryByRole('menu')).not.toBeInTheDocument());
    await waitFor(() => expect(trigger).toHaveFocus());
  });

  it('marks the selected radio item and changes it from the keyboard', async () => {
    const user = userEvent.setup();
    render(<ImageryMenu onCopy={vi.fn()} />);
    await user.click(screen.getByRole('button', { name: 'Imagery' }));
    await screen.findByRole('menu');
    expect(screen.getByRole('menuitemradio', { name: 'Satellite' })).toHaveAttribute(
      'aria-checked',
      'true',
    );
    await user.keyboard('{ArrowDown}');
    await user.keyboard('{ArrowDown}');
    expect(screen.getByRole('menuitemradio', { name: 'Dark streets' })).toHaveFocus();
    await user.keyboard('{Enter}');
    expect(screen.getByRole('menuitemradio', { name: 'Dark streets' })).toHaveAttribute(
      'aria-checked',
      'true',
    );
  });

  it('closes on Escape and returns focus to the trigger', async () => {
    const user = userEvent.setup();
    render(<ImageryMenu onCopy={vi.fn()} />);
    const trigger = screen.getByRole('button', { name: 'Imagery' });
    await user.click(trigger);
    await screen.findByRole('menu');
    await user.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('menu')).not.toBeInTheDocument());
    expect(trigger).toHaveFocus();
  });
});
