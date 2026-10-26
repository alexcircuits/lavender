import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { findNearestCity, getViewBand, HudOverlay } from './HudOverlay';

describe('HudOverlay', () => {
  it('determines appropriate view bands across camera altitude', () => {
    expect(getViewBand(800)).toBe('STREET');
    expect(getViewBand(5000)).toBe('CITY');
    expect(getViewBand(25000)).toBe('METRO');
    expect(getViewBand(120000)).toBe('REGIONAL');
    expect(getViewBand(500000)).toBe('GLOBAL');
  });

  it('locates nearest city preset and bearings', () => {
    // Austin Texas coords
    const result = findNearestCity({ lat: 30.27, lon: -97.74 });
    expect(result).not.toBeNull();
    expect(result?.name).toBe('Austin');
    expect(result?.distanceKm).toBeLessThan(10);
  });

  it('renders telemetry and classification banner in tactical mode', () => {
    render(
      <HudOverlay
        classification="SECRET // NOFORN"
        cameraPosition={{ lat: 30.27, lon: -97.74 }}
        cameraAltitudeM={2500}
        cameraPitchDeg={-30}
      />,
    );

    expect(screen.getByText('SECRET // NOFORN')).toBeInTheDocument();
    expect(screen.getByText('CITY')).toBeInTheDocument();
    expect(screen.getByText(/GSD:/)).toBeInTheDocument();
    expect(screen.getByText(/NIIRS:/)).toBeInTheDocument();
    expect(screen.getByText(/ONA:/)).toBeInTheDocument();
  });

  it('hides corners in minimal layout', () => {
    render(<HudOverlay layout="minimal" classification="UNCLASSIFIED" />);
    expect(screen.getByText('UNCLASSIFIED')).toBeInTheDocument();
    expect(screen.queryByText('BAND:')).not.toBeInTheDocument();
  });
});
