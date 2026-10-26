import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { CyberSonarOverlay } from './CyberSonarOverlay';

describe('CyberSonarOverlay', () => {
  it('renders concentric rings and sweep beam', () => {
    render(<CyberSonarOverlay visible rings={6} />);
    const overlay = screen.getByTestId('cyber-sonar-overlay');
    expect(overlay).toBeInTheDocument();
    // 6 rings
    expect(overlay.querySelectorAll('[class*="ring"]').length).toBe(6);
  });

  it('renders null when not visible', () => {
    render(<CyberSonarOverlay visible={false} />);
    expect(screen.queryByTestId('cyber-sonar-overlay')).not.toBeInTheDocument();
  });
});
