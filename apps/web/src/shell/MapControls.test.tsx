import { act, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { renderWithRuntime } from '../app/testing';
import { clearToasts } from '../design';
import { createRuntime } from '../runtime';
import { workspaceDocumentFromUrl } from '../state/workspaceDocument';
import { FakePlatform, fakeGlobe } from '../test/fakes';
import { MapControls } from './MapControls';

afterEach(() => {
  clearToasts();
  vi.restoreAllMocks();
});

describe('share view control', () => {
  it('waits for globe/catalog readiness and copies the visible workspace with success feedback', async () => {
    const user = userEvent.setup();
    const clipboard = vi.spyOn(navigator.clipboard, 'writeText').mockResolvedValue();
    const platform = new FakePlatform();
    const runtime = createRuntime(platform, {
      workspaceLocation: {
        currentUrl: () => 'https://lavender.example/',
        onNavigate: () => () => {},
      },
    });
    renderWithRuntime(runtime, <MapControls />);
    const share = screen.getByRole('button', { name: 'Copy view link' });
    expect(share).toBeDisabled();
    act(() => {
      runtime.stores.system
        .getState()
        .setCatalog(platform.catalogResult as Awaited<ReturnType<typeof platform.catalog>>);
      runtime.attachGlobe(fakeGlobe().globe);
      runtime.stores.workspace.getState().setLayerEnabled('aircraft', true);
      runtime.stores.workspace.getState().setFilters('aircraft', { militaryOnly: true });
      runtime.stores.workspace.getState().setImagery('night');
    });
    expect(share).toBeEnabled();
    await user.click(share);
    expect(await screen.findByText('Link copied')).toBeVisible();
    const copied = workspaceDocumentFromUrl(clipboard.mock.calls[0]?.[0] ?? '');
    expect(copied?.layers).toContainEqual({
      id: 'aircraft',
      enabled: true,
      options: { militaryOnly: true },
    });
    expect(copied?.imageryId).toBe('night');
    act(() => runtime.detachGlobe());
  });

  it('shows a clipboard failure with the existing notification primitive', async () => {
    const user = userEvent.setup();
    vi.spyOn(navigator.clipboard, 'writeText').mockRejectedValue(new Error('denied'));
    const platform = new FakePlatform();
    const runtime = createRuntime(platform, {
      workspaceLocation: {
        currentUrl: () => 'https://lavender.example/',
        onNavigate: () => () => {},
      },
    });
    runtime.stores.system
      .getState()
      .setCatalog(platform.catalogResult as Awaited<ReturnType<typeof platform.catalog>>);
    runtime.attachGlobe(fakeGlobe().globe);
    renderWithRuntime(runtime, <MapControls />);
    await user.click(screen.getByRole('button', { name: 'Copy view link' }));
    expect(await screen.findByText('Could not copy link')).toBeVisible();
    expect(screen.getByRole('alert')).toHaveTextContent('Allow clipboard access and try again.');
    act(() => runtime.detachGlobe());
  });
});
