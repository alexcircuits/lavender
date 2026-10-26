import { describe, expect, it } from 'vitest';
import { decodeShareHash, encodeShareHash, encodeShareUrl } from './shareCodec';

describe('shareCodec', () => {
  it('round-trips camera pose and workspace state', () => {
    const original = {
      camera: {
        lon: -122.4194,
        lat: 37.7749,
        heightM: 25000,
        headingRad: (45 * Math.PI) / 180,
        pitchRad: (-35 * Math.PI) / 180,
        rollRad: 0,
      },
      enabledLayers: ['aircraft', 'vessels', 'satellites'] as const,
      visualPreset: 'noir' as const,
      hudEnabled: true,
      scopeMaskEnabled: true,
      sonarEnabled: false,
      selection: { feed: 'aircraft' as const, id: 'AFR123' },
      trackingMode: 'cockpit' as const,
      imageryId: 'blue-marble',
    };

    const hash = encodeShareHash(original);
    expect(hash).toContain('v=2');
    expect(hash).toContain('lat=37.7749');
    expect(hash).toContain('lon=-122.4194');
    expect(hash).toContain('alt=25000');
    expect(hash).toContain('heading=45');
    expect(hash).toContain('pitch=-35');
    expect(hash).toContain('style=noir');
    expect(hash).toContain('hud=1');
    expect(hash).toContain('sm=1');
    expect(hash).toContain('sel=aircraft%3AAFR123');
    expect(hash).toContain('mode=cockpit');
    expect(hash).toContain('map=blue-marble');

    const decoded = decodeShareHash(hash);
    expect(decoded).not.toBeNull();
    expect(decoded?.camera?.lat).toBeCloseTo(37.7749, 3);
    expect(decoded?.camera?.lon).toBeCloseTo(-122.4194, 3);
    expect(decoded?.camera?.altM).toBe(25000);
    expect(decoded?.camera?.headingDeg).toBe(45);
    expect(decoded?.camera?.pitchDeg).toBe(-35);
    expect(decoded?.visualPreset).toBe('noir');
    expect(decoded?.hudEnabled).toBe(true);
    expect(decoded?.scopeMaskEnabled).toBe(true);
    expect(decoded?.sonarEnabled).toBe(false);
    expect(decoded?.selection).toEqual({ feed: 'aircraft', id: 'AFR123' });
    expect(decoded?.trackingMode).toBe('cockpit');
    expect(decoded?.imageryId).toBe('blue-marble');
    expect(decoded?.enabledLayers).toEqual(
      expect.arrayContaining(['aircraft', 'vessels', 'satellites']),
    );
  });

  it('strips ?lang parameter from generated URLs to preserve recipient preferences', () => {
    const url = encodeShareUrl(
      {
        camera: {
          lon: 10,
          lat: 20,
          heightM: 50000,
          headingRad: 0,
          pitchRad: -Math.PI / 2,
          rollRad: 0,
        },
      },
      'https://lavender.noir.systems/?lang=fr#existing',
    );

    const parsed = new URL(url);
    expect(parsed.searchParams.get('lang')).toBeNull();
    expect(parsed.hash).toContain('lat=20.0000');
    expect(parsed.hash).toContain('lon=10.0000');
  });

  it('decodes legacy GEV share hashes with backward compatibility', () => {
    // Legacy format with flights (f) and military (m)
    const legacyHash =
      '#lat=40.7128&lon=-74.0060&alt=15000&heading=180&pitch=-45&style=nvg&hv=1&sc=1&l=fm&map=osm';
    const decoded = decodeShareHash(legacyHash);
    expect(decoded).not.toBeNull();
    expect(decoded?.camera?.lat).toBeCloseTo(40.7128, 3);
    expect(decoded?.camera?.lon).toBeCloseTo(-74.006, 3);
    expect(decoded?.camera?.altM).toBe(15000);
    expect(decoded?.camera?.headingDeg).toBe(180);
    expect(decoded?.camera?.pitchDeg).toBe(-45);
    expect(decoded?.visualPreset).toBe('nvg');
    expect(decoded?.hudEnabled).toBe(true);
    expect(decoded?.scopeMaskEnabled).toBe(true);
    expect(decoded?.enabledLayers).toContain('aircraft');
  });

  it('rejects invalid hashes with missing or non-finite coordinates', () => {
    expect(decodeShareHash('')).toBeNull();
    expect(decodeShareHash('#style=noir')).toBeNull();
    expect(decodeShareHash('#lat=Infinity&lon=10')).toBeNull();
    expect(decodeShareHash('#lat=NaN&lon=10')).toBeNull();
  });
});
