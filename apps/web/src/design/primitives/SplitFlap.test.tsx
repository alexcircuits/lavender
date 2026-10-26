import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { SplitFlap } from './SplitFlap';

describe('SplitFlap', () => {
  it('renders text with accessible status label', () => {
    render(<SplitFlap text="BERLIN" minColumns={8} />);
    const status = screen.getByRole('status');
    expect(status).toHaveAttribute('aria-label', 'BERLIN');
    expect(status.children.length).toBe(8);
  });

  it('updates text when prop changes', () => {
    const { rerender } = render(<SplitFlap text="FLIGHT 101" />);
    expect(screen.getByRole('status')).toHaveAttribute('aria-label', 'FLIGHT 101');
    rerender(<SplitFlap text="ARRIVED" />);
    expect(screen.getByRole('status')).toHaveAttribute('aria-label', 'ARRIVED');
  });
});
