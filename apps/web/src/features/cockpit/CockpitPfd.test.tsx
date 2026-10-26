import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { CockpitPfd } from './CockpitPfd';

describe('CockpitPfd', () => {
  it('renders PFD instruments and telemetry', () => {
    render(
      <CockpitPfd
        callsign="AFR123"
        registration="F-GZCP"
        typeCode="A332"
        headingDeg={270}
        speedKnots={480}
        altitudeFt={36000}
        squawk="7700"
      />,
    );

    expect(screen.getByTestId('cockpit-pfd')).toBeInTheDocument();
    expect(screen.getByText('270°')).toBeInTheDocument();
    expect(screen.getByText('480')).toBeInTheDocument();
    expect(screen.getByText('36,000')).toBeInTheDocument();
    expect(screen.getByText('AFR123')).toBeInTheDocument();
    expect(screen.getByText('7700')).toBeInTheDocument();
  });

  it('handles exit cockpit button click', () => {
    const onExit = vi.fn();
    render(<CockpitPfd onExit={onExit} />);
    const exitBtn = screen.getByRole('button', { name: /exit cockpit/i });
    fireEvent.click(exitBtn);
    expect(onExit).toHaveBeenCalledTimes(1);
  });
});
