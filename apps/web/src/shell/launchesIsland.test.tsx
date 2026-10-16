/**
 * The launch list as a left island in the real shell: opened from the launches layer's
 * options and the command palette, back to the layers, and focus kept on the rail when it
 * closes (it has no rail button of its own: it belongs to the Layers island).
 */
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { App as AppRoot } from '../app/App';
import { type Harness, startHarness } from '../app/testing';
import type { MountGlobe } from '../globe';
import { catalogOf, fakeGlobe, layer } from '../test/fakes';

const CATALOG = catalogOf([layer('launches', { name: 'Rocket launches', family: 'space' })]);

let harness: Harness | null = null;

async function setup(options: Parameters<typeof startHarness>[0] = { catalog: CATALOG }) {
  harness = await startHarness(options);
  const { globe } = fakeGlobe();
  const mountGlobe = vi.fn<MountGlobe>(async () => globe);
  render(<AppRoot runtime={harness.runtime} mountGlobe={mountGlobe} />);
  return { ...harness, user: userEvent.setup() };
}

afterEach(() => {
  harness?.stop();
  harness = null;
});

describe('launch list island', () => {
  it('opens from the layer options, goes back to the layers and returns focus to the rail', async () => {
    const h = await setup();
    const rail = screen.getByRole('navigation', { name: 'Workspace' });
    await h.user.click(await screen.findByRole('button', { name: 'Rocket launches options' }));
    await h.user.click(screen.getByRole('button', { name: 'Show launch list' }));
    const island = screen.getByRole('region', { name: 'Rocket launches' });
    expect(within(island).getByText(/The Rocket launches layer is off/)).toBeInTheDocument();

    await h.user.click(within(island).getByRole('button', { name: 'Show layers' }));
    expect(screen.getByRole('region', { name: 'Layers' })).toBeInTheDocument();
    await h.user.click(screen.getByRole('button', { name: 'Rocket launches options' }));
    await h.user.click(screen.getByRole('button', { name: 'Show launch list' }));

    // Closing it from inside: focus lands on the Layers rail button, which reopens the layers.
    await h.user.click(screen.getByRole('button', { name: 'Close rocket launches' }));
    expect(screen.queryByRole('region', { name: 'Rocket launches' })).not.toBeInTheDocument();
    await waitFor(() => expect(within(rail).getByRole('button', { name: 'Layers' })).toHaveFocus());
  });

  it('opens from the command palette', async () => {
    const h = await setup();
    await h.user.keyboard('{Control>}k{/Control}');
    const palette = await screen.findByRole('dialog', { name: 'Search and commands' });
    await h.user.keyboard('show rocket');
    await h.user.click(within(palette).getByRole('option', { name: /Show rocket launches/ }));
    expect(await screen.findByRole('region', { name: 'Rocket launches' })).toBeInTheDocument();
  });

  it('names itself while the catalog loads', async () => {
    harness = await startHarness({ start: false });
    const { globe } = fakeGlobe();
    render(<AppRoot runtime={harness.runtime} mountGlobe={async () => globe} />);
    harness.runtime.stores.ui.getState().setSidePanel('launches');
    expect(await screen.findByRole('region', { name: 'Rocket launches' })).toHaveAttribute(
      'aria-busy',
      'true',
    );
  });
});
