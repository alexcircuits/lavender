import { act, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { deliver, type Harness, renderWithRuntime, source, startHarness } from '../../app/testing';
import { AT_REST_KNOTS, THIN_MIN_HEIGHT_M } from '../../globe/renderers/vesselSymbology';
import { catalogOf, layer } from '../../test/fakes';
import { vessel, vesselSnapshot } from '../../test/vessel';
import { LayerExplorer } from './LayerExplorer';
import {
  ALL_DRAWN_NOW,
  AT_REST_LABEL,
  THINNING_ABOVE_M,
  THINNING_NOTE,
  THINNING_NOW,
  UNDERWAY_LABEL,
} from './VesselLegend';

const CATALOG = catalogOf(
  [layer('vessels', { name: 'Vessels', family: 'maritime', sourceIds: ['aisstream'] })],
  { sources: [source('aisstream', { name: 'AISStream' })] },
);

let harness: Harness | null = null;

afterEach(() => {
  harness?.stop();
  harness = null;
});

describe('Vessels layer', () => {
  it('sits under Maritime with an honest count, freshness and a chevron/dot legend', async () => {
    harness = await startHarness({ catalog: CATALOG });
    const h = harness;
    const user = userEvent.setup();
    renderWithRuntime(h.runtime, <LayerExplorer catalog={CATALOG} />);
    const group = screen.getByRole('list', { name: 'Maritime layers' });
    const row = within(group).getByRole('switch', { name: 'Vessels' });
    await user.click(row);
    expect(within(group).getByText('Connecting…')).toBeInTheDocument();

    deliver(h, {
      ...vesselSnapshot([vessel({ mmsi: '230990001' }), vessel({ mmsi: '230990002' })]),
      sourceCount: 5,
      fetchedAt: new Date().toISOString(),
    });
    expect(within(group).getByText('2')).toBeInTheDocument();
    expect(within(group).getByText('vessels')).toHaveClass('visually-hidden');

    await user.click(screen.getByRole('button', { name: 'Vessels options' }));
    const legend = screen.getByRole('list', { name: 'Vessel symbol legend' });
    expect(within(legend).getByText(UNDERWAY_LABEL)).toBeInTheDocument();
    expect(within(legend).getByText(AT_REST_LABEL)).toBeInTheDocument();
    expect(screen.getByText('2 in view · 5 known to the server')).toBeInTheDocument();
    expect(screen.getByText(THINNING_NOTE)).toBeInTheDocument();

    deliver(h, { ...vesselSnapshot([]), freshness: { state: 'stale', ageS: 900 } });
    expect(within(group).getByText('Stale')).toBeInTheDocument();
    expect(screen.getByText('0 in view · 0 known to the server')).toBeInTheDocument();
  });

  it('keeps the legend wording in step with the at-rest speed the globe uses', () => {
    expect(AT_REST_LABEL).toContain(`under ${AT_REST_KNOTS} kn`);
    expect(THINNING_ABOVE_M).toBe(THIN_MIN_HEIGHT_M);
  });

  it('says whether the current view thins ships, while the count stays every ship', async () => {
    harness = await startHarness({ catalog: CATALOG, globe: true });
    const h = harness;
    const user = userEvent.setup();
    renderWithRuntime(h.runtime, <LayerExplorer catalog={CATALOG} />);
    await user.click(screen.getByRole('switch', { name: 'Vessels' }));
    await user.click(screen.getByRole('button', { name: 'Vessels options' }));
    // The fake globe starts at its 20 000 km home pose.
    expect(screen.getByText(THINNING_NOW)).toBeInTheDocument();
    const view = (heightM: number) =>
      act(() => h.globe?.view({ bbox: null, centre: { lat: 55, lon: 4 }, heightM }));
    view(20_000_000);
    expect(screen.getByText(THINNING_NOW)).toBeInTheDocument();
    view(THIN_MIN_HEIGHT_M);
    expect(screen.getByText(ALL_DRAWN_NOW)).toBeInTheDocument();
  });

  it('says why the layer is off when the key is missing', async () => {
    const catalog = catalogOf(
      [
        layer('vessels', {
          name: 'Vessels',
          family: 'maritime',
          availability: { state: 'unconfigured', missing: ['LAVENDER_AISSTREAM_API_KEY'] },
        }),
      ],
      { sources: [source('aisstream', { name: 'AISStream' })] },
    );
    harness = await startHarness({ catalog });
    renderWithRuntime(harness.runtime, <LayerExplorer catalog={catalog} />);
    expect(screen.getByRole('switch', { name: 'Vessels' })).toHaveAttribute(
      'aria-disabled',
      'true',
    );
    expect(screen.getByText('Needs LAVENDER_AISSTREAM_API_KEY')).toBeInTheDocument();
    act(() => harness?.runtime.stores.workspace.getState().setLayerEnabled('vessels', true));
    expect(harness.platform.live('vessels')).toBeUndefined();
  });
});
