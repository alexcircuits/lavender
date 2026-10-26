import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { ScopeMask } from './ScopeMask';

describe('ScopeMask', () => {
  it('renders scope mask canvas element', () => {
    render(<ScopeMask enabled cameraAltitudeM={5_000_000} />);
    const mask = screen.getByTestId('scope-mask');
    expect(mask).toBeInTheDocument();
    expect(mask.querySelector('canvas')).toBeInTheDocument();
  });

  it('updates when camera altitude crosses terminus threshold', () => {
    const { rerender } = render(<ScopeMask enabled cameraAltitudeM={12_000_000} />);
    expect(screen.getByTestId('scope-mask')).toBeInTheDocument();
    rerender(<ScopeMask enabled cameraAltitudeM={6_000_000} />);
    expect(screen.getByTestId('scope-mask')).toBeInTheDocument();
  });
});
