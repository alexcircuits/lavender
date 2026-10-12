import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { Button } from './Button';
import { Dialog, DialogClose } from './Dialog';
import { TextField } from './TextField';

function KeyDialog({ destructive = false, onConfirm = vi.fn() }) {
  return (
    <>
      <Button>Before</Button>
      <Dialog
        trigger={<Button>Add a key</Button>}
        title="Add a FIRMS key"
        description="Fires need a NASA FIRMS map key. It stays on this device."
        destructive={destructive}
        actions={
          <>
            <DialogClose render={<Button variant="secondary">Cancel</Button>} />
            <Button variant="primary" onClick={onConfirm}>
              Save key
            </Button>
          </>
        }
      >
        <TextField label="Map key" mono />
      </Dialog>
    </>
  );
}

describe('Dialog', () => {
  it('is a named modal dialog that focuses its first field when opened', async () => {
    const user = userEvent.setup();
    render(<KeyDialog />);
    await user.click(screen.getByRole('button', { name: 'Add a key' }));
    const dialog = await screen.findByRole('dialog', { name: 'Add a FIRMS key' });
    expect(dialog).toHaveAccessibleDescription(/NASA FIRMS map key/);
    await waitFor(() => expect(screen.getByRole('textbox', { name: 'Map key' })).toHaveFocus());
  });

  it('traps focus: Tab and Shift+Tab cycle inside the dialog', async () => {
    const user = userEvent.setup();
    render(<KeyDialog />);
    await user.click(screen.getByRole('button', { name: 'Add a key' }));
    const dialog = await screen.findByRole('dialog');
    await waitFor(() => expect(dialog.contains(document.activeElement)).toBe(true));
    for (let i = 0; i < 8; i += 1) {
      await user.tab();
      expect(dialog.contains(document.activeElement)).toBe(true);
    }
    for (let i = 0; i < 8; i += 1) {
      await user.tab({ shift: true });
      expect(dialog.contains(document.activeElement)).toBe(true);
    }
    // Outside content is hidden from assistive technology while the modal is open.
    expect(screen.queryByRole('button', { name: 'Before' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Before', hidden: true })).not.toHaveFocus();
  });

  it('closes on Escape and returns focus to the trigger', async () => {
    const user = userEvent.setup();
    render(<KeyDialog />);
    const trigger = screen.getByRole('button', { name: 'Add a key' });
    await user.click(trigger);
    await screen.findByRole('dialog');
    await user.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    await waitFor(() => expect(trigger).toHaveFocus());
  });

  it('a destructive dialog ignores Escape and waits for a choice', async () => {
    const user = userEvent.setup();
    render(<KeyDialog destructive />);
    const trigger = screen.getByRole('button', { name: 'Add a key' });
    await user.click(trigger);
    await screen.findByRole('dialog');
    expect(screen.queryByRole('button', { name: 'Close' })).not.toBeInTheDocument();
    await user.keyboard('{Escape}');
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    await waitFor(() => expect(trigger).toHaveFocus());
  });
});
