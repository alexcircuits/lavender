import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { SegmentedControl } from './SegmentedControl';

const MODES = [
  { value: 'globe', label: '3D globe' },
  { value: 'flat', label: '2D map' },
  { value: 'columbus', label: 'Columbus' },
] as const;

describe('SegmentedControl', () => {
  it('is a named radio group with one checked option', () => {
    render(<SegmentedControl aria-label="Globe mode" options={MODES} defaultValue="globe" />);
    expect(screen.getByRole('radiogroup', { name: 'Globe mode' })).toBeInTheDocument();
    expect(screen.getAllByRole('radio')).toHaveLength(3);
    expect(screen.getByRole('radio', { name: '3D globe' })).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByRole('radio', { name: '2D map' })).toHaveAttribute('aria-checked', 'false');
  });

  it('moves the selection with arrow keys', async () => {
    const user = userEvent.setup();
    const onValueChange = vi.fn();
    render(
      <SegmentedControl
        aria-label="Globe mode"
        options={MODES}
        defaultValue="globe"
        onValueChange={onValueChange}
      />,
    );
    await user.tab();
    expect(screen.getByRole('radio', { name: '3D globe' })).toHaveFocus();
    await user.keyboard('{ArrowRight}');
    expect(screen.getByRole('radio', { name: '2D map' })).toHaveFocus();
    expect(screen.getByRole('radio', { name: '2D map' })).toHaveAttribute('aria-checked', 'true');
    expect(onValueChange).toHaveBeenLastCalledWith('flat');
  });

  it('selects on click', async () => {
    const user = userEvent.setup();
    const onValueChange = vi.fn();
    render(
      <SegmentedControl
        aria-label="Globe mode"
        options={MODES}
        defaultValue="globe"
        onValueChange={onValueChange}
      />,
    );
    await user.click(screen.getByRole('radio', { name: 'Columbus' }));
    expect(onValueChange).toHaveBeenLastCalledWith('columbus');
  });
});
