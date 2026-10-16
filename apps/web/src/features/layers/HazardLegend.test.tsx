import { render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { cycloneRampIndex } from '../../globe/renderers/cyclones';
import { cycloneSpiral } from '../../globe/renderers/hazardShapes';
import { fireRampIndex, quakeRampIndex } from '../../globe/renderers/symbology';
import { HazardLegend } from './HazardLegend';
import styles from './layers.module.css';

type Step = 0 | 1 | 2 | 3 | 4 | null;

/**
 * Each legend's labels and the four boundaries between its five ramp steps, checked against
 * the function the globe colours symbols with. Continuous values (FRP, magnitude) use
 * "a–<b" bins; cyclone winds are whole knots, so "34–63 kn" is exact.
 */
const LEGENDS: readonly {
  readonly kind: 'fire' | 'magnitude' | 'wind';
  readonly name: string;
  readonly labels: readonly string[];
  readonly boundaries: readonly [number, number, number, number];
  readonly step: (value: number) => Step;
}[] = [
  {
    kind: 'fire',
    name: 'Active fire intensity legend',
    labels: ['<10 MW', '10–<50 MW', '50–<100 MW', '100–<250 MW', '≥250 MW'],
    boundaries: [10, 50, 100, 250],
    step: fireRampIndex,
  },
  {
    kind: 'magnitude',
    name: 'Earthquake magnitude legend',
    labels: ['<M3', 'M3–<4', 'M4–<5', 'M5–<6', '≥M6'],
    boundaries: [3, 4, 5, 6],
    step: quakeRampIndex,
  },
  {
    kind: 'wind',
    name: 'Cyclone sustained-wind legend',
    labels: ['<34 kn', '34–63 kn', '64–95 kn', '96–136 kn', '≥137 kn'],
    boundaries: [34, 64, 96, 137],
    step: cycloneRampIndex,
  },
];

describe('HazardLegend', () => {
  it.each(LEGENDS)('labels the $kind ramp steps at the boundaries the globe uses', (legend) => {
    render(<HazardLegend kind={legend.kind} />);
    const items = within(screen.getByRole('list', { name: legend.name })).getAllByRole('listitem');
    const ramp = items.slice(0, 5);
    expect(ramp.map((item) => item.textContent)).toEqual(legend.labels);
    ramp.forEach((item, step) => {
      expect(item.querySelector('[data-step]')?.getAttribute('data-step')).toBe(String(step));
    });
    legend.boundaries.forEach((boundary, index) => {
      expect(legend.step(boundary - 0.01), `just below ${boundary}`).toBe(index);
      expect(legend.step(boundary), `at ${boundary}`).toBe(index + 1);
    });
  });

  it('keys each hazard with the shape the globe draws: quake rings, fire cells, cyclone spirals', () => {
    const shapes = (kind: 'fire' | 'magnitude' | 'wind') => {
      const { container, unmount } = render(<HazardLegend kind={kind} />);
      const found = [...container.querySelectorAll('[data-step]')].map((swatch) =>
        swatch.getAttribute('data-shape'),
      );
      unmount();
      return new Set(found);
    };
    expect(shapes('magnitude')).toEqual(new Set(['ring']));
    expect(shapes('fire')).toEqual(new Set(['cell']));
    expect(shapes('wind')).toEqual(new Set(['spiral']));
  });

  it('draws every swatch inside the hazard edge: outline, keyline, then the ramp colour', () => {
    for (const kind of ['fire', 'magnitude', 'wind'] as const) {
      const { container, unmount } = render(<HazardLegend kind={kind} />);
      for (const swatch of container.querySelectorAll('svg[data-step]')) {
        const layers = [...swatch.children].map((layer) => layer.getAttribute('class'));
        expect(layers, kind).toEqual([
          styles.legendOutline,
          styles.legendKeyline,
          styles.legendFill,
        ]);
      }
      unmount();
    }
    // A ring stays open: its centre is never filled.
    const { container } = render(<HazardLegend kind="magnitude" />);
    for (const ring of container.querySelectorAll('circle')) {
      expect(ring.getAttribute('fill')).toBe('none');
    }
  });

  it('draws the cyclone swatch from the globe glyph geometry (eye and two arms)', () => {
    const { container } = render(<HazardLegend kind="wind" />);
    const fill = container.querySelector(`svg[data-step="2"] .${styles.legendFill}`);
    const arms = fill?.querySelectorAll('path') ?? [];
    expect(arms).toHaveLength(cycloneSpiral('north').arms.length);
    expect(fill?.querySelectorAll('circle')).toHaveLength(1);
    // Northern storms turn counter-clockwise and their arms trail: the first arm leaves the
    // eye's top towards the right.
    const [start, next] = cycloneSpiral('north').arms[0] ?? [];
    expect(start?.[1]).toBeLessThan(0);
    expect(next?.[0]).toBeGreaterThan(start?.[0] ?? 0);
  });

  it('keys unknown cyclone wind separately from the ramp', () => {
    render(<HazardLegend kind="wind" />);
    const items = within(
      screen.getByRole('list', { name: 'Cyclone sustained-wind legend' }),
    ).getAllByRole('listitem');
    expect(items).toHaveLength(6);
    expect(items[5]?.textContent).toBe('Wind not reported');
    expect(items[5]?.querySelector('[data-step]')?.getAttribute('data-step')).toBe('unknown');
    expect(cycloneRampIndex(undefined)).toBeNull();
  });
});
