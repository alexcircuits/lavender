import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { MountGlobe } from '../globe';
import { createRuntime } from '../runtime';
import { SHARE_BREAKPOINT_PX } from '../shell';
import { catalogOf, FakePlatform, layer, fakeGlobe as recordingGlobe } from '../test/fakes';
import { App as AppRoot, DESIGN_REVIEW_HASH } from './App';
import { aircraft, aircraftSnapshot, deliver, type Harness, startHarness } from './testing';

function fakeGlobe() {
  const { globe } = recordingGlobe();
  const mountGlobe = vi.fn<MountGlobe>(async () => globe);
  return { mountGlobe, destroy: globe.destroy, globe };
}

const runtime = createRuntime(new FakePlatform());
function App({ mountGlobe }: { mountGlobe: MountGlobe }) {
  return <AppRoot runtime={runtime} mountGlobe={mountGlobe} />;
}

describe('App', () => {
  afterEach(() => {
    window.location.hash = '';
  });

  it('paints the shell immediately and mounts the globe into its host', async () => {
    const { mountGlobe } = fakeGlobe();
    render(<App mountGlobe={mountGlobe} />);
    expect(screen.getByRole('main', { name: 'Globe' })).toBeInTheDocument();
    expect(screen.getByText('Connecting…')).toBeInTheDocument();
    expect(screen.getByText(/UTC$/)).toBeInTheDocument();
    await waitFor(() => expect(mountGlobe).toHaveBeenCalledOnce());
    const [options] = mountGlobe.mock.calls[0] ?? [];
    expect(options?.host).toHaveClass('lv-globe-host');
    await waitFor(() => expect(screen.queryByText('Loading globe…')).not.toBeInTheDocument());
  });

  it('says so when the globe cannot start', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const mountGlobe: MountGlobe = async () => {
      throw new Error('WebGL2 is not available');
    };
    render(<App mountGlobe={mountGlobe} />);
    const main = screen.getByRole('main', { name: 'Globe' });
    expect(await within(main).findByRole('alert')).toHaveTextContent('The globe could not start.');
  });

  it('destroys the globe when the shell unmounts', async () => {
    const { mountGlobe, destroy } = fakeGlobe();
    const { unmount } = render(<App mountGlobe={mountGlobe} />);
    await waitFor(() => expect(mountGlobe).toHaveBeenCalled());
    await waitFor(() => expect(screen.queryByText('Loading globe…')).not.toBeInTheDocument());
    unmount();
    expect(destroy).toHaveBeenCalled();
  });

  it('renders the design review on #design-review in development', async () => {
    const { mountGlobe } = fakeGlobe();
    render(<App mountGlobe={mountGlobe} />);
    act(() => {
      window.location.hash = DESIGN_REVIEW_HASH;
      window.dispatchEvent(new HashChangeEvent('hashchange'));
    });
    expect(
      await screen.findByRole('heading', { level: 1, name: 'Design review' }, { timeout: 5000 }),
    ).toBeInTheDocument();
    expect(screen.getAllByRole('region', { name: 'Layers' }).length).toBeGreaterThan(0);
    expect(screen.getByRole('searchbox', { name: 'Filter layers' })).toBeInTheDocument();
    const airLayers = screen.getByRole('list', { name: 'Air layers' });
    expect(within(airLayers).getByRole('switch', { name: 'Aircraft' })).toHaveAttribute(
      'aria-checked',
      'true',
    );
    expect(screen.getByRole('table', { name: 'Earthquakes, past 7 days' })).toBeInTheDocument();
    for (const word of ['Live', 'Stale', 'Degraded', 'Unavailable', 'Not configured', 'Licence']) {
      expect(screen.getAllByText(word).length).toBeGreaterThan(0);
    }
  });
});

describe('App shell', () => {
  let harness: Harness | null = null;

  afterEach(() => {
    harness?.stop();
    harness = null;
  });

  async function setup(options: Parameters<typeof startHarness>[0] = {}) {
    harness = await startHarness(options);
    const { mountGlobe, globe } = fakeGlobe();
    const h = harness;
    render(<AppRoot runtime={h.runtime} mountGlobe={mountGlobe} />);
    return { ...h, mountedGlobe: globe, user: userEvent.setup() };
  }

  it('shows a skeleton island while the catalog loads', async () => {
    harness = await startHarness({ start: false });
    const { mountGlobe } = fakeGlobe();
    render(<AppRoot runtime={harness.runtime} mountGlobe={mountGlobe} />);
    expect(screen.getByRole('region', { name: 'Layers' })).toHaveAttribute('aria-busy', 'true');
    await waitFor(() => expect(mountGlobe).toHaveBeenCalled());
  });

  it('says when the catalog cannot load and retries on request', async () => {
    const h = await setup({ catalog: new Error('backend down') });
    const island = await screen.findByRole('region', { name: 'Layers' });
    expect(within(island).getByText('Layers could not load')).toBeInTheDocument();
    h.platform.catalogResult = catalogOf([layer('earthquakes', { name: 'Earthquakes' })]);
    await h.user.click(within(island).getByRole('button', { name: 'Retry' }));
    expect(await screen.findByRole('switch', { name: 'Earthquakes' })).toBeInTheDocument();
  });

  it('switches the left island from the rail and closes it on a second press', async () => {
    const h = await setup();
    const rail = screen.getByRole('navigation', { name: 'Workspace' });
    const layersButton = within(rail).getByRole('button', { name: 'Layers' });
    expect(layersButton).toHaveAttribute('aria-pressed', 'true');
    await h.user.click(within(rail).getByRole('button', { name: 'Sources' }));
    expect(screen.getByRole('region', { name: 'Sources' })).toBeInTheDocument();
    expect(within(rail).getByRole('button', { name: 'Sources' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    await h.user.click(within(rail).getByRole('button', { name: 'Sources' }));
    expect(screen.queryByRole('region', { name: 'Sources' })).not.toBeInTheDocument();
    await h.user.click(within(rail).getByRole('button', { name: 'Settings' }));
    expect(screen.getByRole('region', { name: 'Settings' })).toBeInTheDocument();
  });

  it('keeps map controls disabled until the globe mounts, then drives the camera', async () => {
    harness = await startHarness();
    const { globe } = fakeGlobe();
    let resolve: (value: typeof globe) => void = () => undefined;
    const mountGlobe = vi.fn<MountGlobe>(() => new Promise((r) => (resolve = r)));
    render(<AppRoot runtime={harness.runtime} mountGlobe={mountGlobe} />);
    const controls = screen.getByRole('toolbar', { name: 'Map controls' });
    expect(within(controls).getByRole('button', { name: 'Zoom in' })).toBeDisabled();
    await act(async () => resolve(globe));
    const zoomIn = within(controls).getByRole('button', { name: 'Zoom in' });
    expect(zoomIn).toBeEnabled();
    await userEvent.setup().click(zoomIn);
    expect(globe.zoomIn).toHaveBeenCalledOnce();
  });

  it('opens Sources from the feed chip and the credits control', async () => {
    const h = await setup();
    act(() => h.runtime.stores.ui.getState().setSidePanel(null));
    await h.user.click(screen.getByRole('button', { name: /^Feeds/ }));
    expect(h.runtime.stores.ui.getState().sidePanel).toBe('sources');
    act(() => h.runtime.stores.ui.getState().setSidePanel(null));
    await h.user.click(screen.getByRole('button', { name: /^Data sources.*show attribution$/ }));
    expect(h.runtime.stores.ui.getState().sidePanel).toBe('sources');
  });

  it('lets the island opened last win when both would crowd the globe', async () => {
    // happy-dom's window is 1024 px wide: below the 1 190 px sharing breakpoint.
    expect(window.matchMedia(`(max-width: ${SHARE_BREAKPOINT_PX - 1}px)`).matches).toBe(true);
    const h = await setup();
    act(() => h.runtime.stores.workspace.getState().setLayerEnabled('aircraft', true));
    deliver(h, aircraftSnapshot([aircraft('4aca2d', { callsign: 'SAS1462' })]));
    expect(screen.getByRole('region', { name: 'Layers' })).toBeInTheDocument();

    act(() => h.runtime.stores.workspace.getState().select({ feed: 'aircraft', id: '4aca2d' }));
    expect(screen.getByRole('region', { name: 'SAS1462' })).toBeInTheDocument();
    expect(screen.queryByRole('region', { name: 'Layers' })).not.toBeInTheDocument();

    const rail = screen.getByRole('navigation', { name: 'Workspace' });
    await h.user.click(within(rail).getByRole('button', { name: 'Sources' }));
    expect(screen.getByRole('region', { name: 'Sources' })).toBeInTheDocument();
    expect(screen.queryByRole('region', { name: 'SAS1462' })).not.toBeInTheDocument();
    // The selection is kept; only the island folds away.
    expect(h.runtime.stores.workspace.getState().selection).not.toBeNull();
  });

  it('returns focus to the rail when the left island closes or switches', async () => {
    const h = await setup({ catalog: catalogOf([layer('earthquakes', { name: 'Earthquakes' })]) });
    const rail = screen.getByRole('navigation', { name: 'Workspace' });
    await screen.findByRole('switch', { name: 'Earthquakes' });

    // The footer's "Sources" switches panels: focus lands on the rail's Sources button.
    const layersIsland = screen.getByRole('region', { name: 'Layers' });
    await h.user.click(within(layersIsland).getByRole('button', { name: 'Sources' }));
    await waitFor(() =>
      expect(within(rail).getByRole('button', { name: 'Sources' })).toHaveFocus(),
    );

    // Closing an island from inside it: focus lands on the rail button that reopens it.
    await h.user.click(screen.getByRole('button', { name: 'Close sources' }));
    expect(screen.queryByRole('region', { name: 'Sources' })).not.toBeInTheDocument();
    await waitFor(() =>
      expect(within(rail).getByRole('button', { name: 'Sources' })).toHaveFocus(),
    );
  });

  it('opens the palette with Ctrl K anywhere in the app', async () => {
    const h = await setup();
    await h.user.keyboard('{Control>}k{/Control}');
    expect(await screen.findByRole('dialog', { name: 'Search and commands' })).toBeInTheDocument();
  });
});
