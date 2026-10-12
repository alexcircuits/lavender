import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { CrosshairIcon, PlusIcon } from '../icons';
import { Button } from './Button';
import { IconButton } from './IconButton';

describe('Button', () => {
  it('is a native button named by its label, with variant and size as data attributes', () => {
    render(
      <Button variant="primary" size="lg" icon={CrosshairIcon}>
        Track
      </Button>,
    );
    const button = screen.getByRole('button', { name: 'Track' });
    expect(button).toHaveAttribute('type', 'button');
    expect(button).toHaveAttribute('data-variant', 'primary');
    expect(button).toHaveAttribute('data-size', 'lg');
    // The icon is decoration: hidden from assistive technology.
    expect(button.querySelector('svg')).toHaveAttribute('aria-hidden', 'true');
  });

  it('activates with Enter and Space from the keyboard', async () => {
    const user = userEvent.setup();
    const onClick = vi.fn();
    render(<Button onClick={onClick}>Copy coordinates</Button>);
    await user.tab();
    expect(screen.getByRole('button', { name: 'Copy coordinates' })).toHaveFocus();
    await user.keyboard('{Enter}');
    await user.keyboard(' ');
    expect(onClick).toHaveBeenCalledTimes(2);
  });

  it('does nothing while disabled and says so with data-disabled', async () => {
    const user = userEvent.setup();
    const onClick = vi.fn();
    render(
      <Button disabled onClick={onClick}>
        Follow
      </Button>,
    );
    const button = screen.getByRole('button', { name: 'Follow' });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute('data-disabled');
    await user.click(button);
    expect(onClick).not.toHaveBeenCalled();
  });
});

describe('IconButton', () => {
  it('takes its accessible name from the required label', () => {
    render(<IconButton label="Zoom in" icon={PlusIcon} />);
    expect(screen.getByRole('button', { name: 'Zoom in' })).toBeInTheDocument();
  });

  it('shows the label (and key hint) as a tooltip when focused from the keyboard', async () => {
    const user = userEvent.setup();
    render(<IconButton label="Zoom in" icon={PlusIcon} shortcut="+" />);
    await user.tab();
    expect(screen.getByRole('button', { name: 'Zoom in' })).toHaveFocus();
    expect(await screen.findByText('Zoom in')).toBeVisible();
    expect(screen.getByText('+').tagName).toBe('KBD');
  });

  it('exposes pressed state for toggle buttons', async () => {
    const user = userEvent.setup();
    const onClick = vi.fn();
    render(<IconButton label="Follow" icon={CrosshairIcon} pressed onClick={onClick} />);
    const button = screen.getByRole('button', { name: 'Follow' });
    expect(button).toHaveAttribute('aria-pressed', 'true');
    await user.click(button);
    expect(onClick).toHaveBeenCalledOnce();
  });
});
