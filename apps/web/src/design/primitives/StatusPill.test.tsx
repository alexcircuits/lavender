import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { type FeedStatus, STATUS_SPECS, StatusPill } from './StatusPill';

const EXPECTED: Record<FeedStatus, string> = {
  live: 'Live',
  stale: 'Stale',
  degraded: 'Degraded',
  unavailable: 'Unavailable',
  'not-configured': 'Not configured',
  licence: 'Licence',
};

describe('StatusPill', () => {
  it.each(Object.entries(EXPECTED))(
    'renders the word for %s, never colour alone',
    (status, word) => {
      const { container } = render(<StatusPill status={status as FeedStatus} />);
      expect(screen.getByText(word)).toBeInTheDocument();
      const glyph = container.querySelector('svg');
      expect(glyph).not.toBeNull();
      expect(glyph).toHaveAttribute('aria-hidden', 'true');
      expect(container.firstElementChild).toHaveAttribute('data-status', status);
    },
  );

  it('covers every status the type allows', () => {
    expect(Object.keys(STATUS_SPECS).sort()).toEqual(Object.keys(EXPECTED).sort());
  });

  it('appends a qualifier to the word', () => {
    const { container } = render(<StatusPill status="stale" detail="4 min" />);
    expect(container.textContent).toBe('Stale 4 min');
  });

  it('gives each state a distinct glyph or weight, so shape alone also separates them', () => {
    const shapes = Object.values(STATUS_SPECS).map(
      (spec) => `${spec.icon.displayName}:${spec.weight}`,
    );
    expect(new Set(shapes).size).toBe(shapes.length);
  });
});
