import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { type Harness, renderWithRuntime, startHarness } from '../../app/testing';
import type { SavedSearch, SavedSearchMatch } from '../../contracts';
import { AlertsPanel } from './AlertsPanel';

const harnesses: Harness[] = [];

async function setup(initialSearches: readonly SavedSearch[] = []) {
  const h = await startHarness({ globe: true });
  harnesses.push(h);

  let storedSearches = [...initialSearches];
  h.platform.listSavedSearches.mockImplementation(async () => storedSearches);
  h.platform.createSavedSearch.mockImplementation(async (search) => {
    storedSearches = [...storedSearches, search];
    return search;
  });
  h.platform.deleteSavedSearch.mockImplementation(async (id) => {
    storedSearches = storedSearches.filter((s) => s.id !== id);
  });

  const view = renderWithRuntime(h.runtime, <AlertsPanel />);
  const user = userEvent.setup();
  return { h, view, user };
}

describe('SavedSearchesTab (LB-140 Item 5)', () => {
  afterEach(() => {
    for (const h of harnesses.splice(0)) {
      h.stop();
    }
  });

  it('switches from Inbox to Saved Searches tab in AlertsPanel', async () => {
    const { user } = await setup();

    const savedSearchesRadio = screen.getByRole('radio', { name: 'Saved Searches' });
    await user.click(savedSearchesRadio);

    expect(screen.getByLabelText('Saved search name')).toBeTruthy();
    expect(screen.getByLabelText('Saved search query')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Save Search' })).toBeTruthy();
  });

  it('creates a new saved search via platform.createSavedSearch and lists it', async () => {
    const { h, user } = await setup();

    await user.click(screen.getByRole('radio', { name: 'Saved Searches' }));

    const nameInput = screen.getByLabelText('Saved search name');
    const queryInput = screen.getByLabelText('Saved search query');

    await user.type(nameInput, 'US Navy Patrol');
    await user.type(queryInput, 'P8');
    await user.click(screen.getByRole('button', { name: 'Save Search' }));

    expect(h.platform.createSavedSearch).toHaveBeenCalledTimes(1);
    expect(h.platform.createSavedSearch).toHaveBeenCalledWith(
      expect.objectContaining({
        name: 'US Navy Patrol',
        query: 'P8',
        notifyOnMatch: true,
      }),
    );

    await waitFor(() => {
      expect(screen.getByText('US Navy Patrol')).toBeTruthy();
    });
    expect(screen.getByText('P8')).toBeTruthy();
    expect(screen.getByText('ALERT ON')).toBeTruthy();
  });

  it('lists saved searches, loads matches via platform.savedSearchMatches, and focuses a match', async () => {
    const existingSearch: SavedSearch = {
      id: 'search-tankers',
      name: 'Atlantic Tankers',
      query: 'TANKER',
      notifyOnMatch: true,
      createdAt: '2026-10-25T12:00:00Z',
    };

    const sampleMatches: readonly SavedSearchMatch[] = [
      {
        searchId: 'search-tankers',
        searchName: 'Atlantic Tankers',
        feed: 'vessels',
        entityId: 'mmsi-211000999',
        label: 'MT NORDIC STAR',
        matchedAt: '2026-10-25T12:05:00Z',
      },
    ];

    const { h, user } = await setup([existingSearch]);
    h.platform.savedSearchMatches.mockResolvedValue(sampleMatches);

    await user.click(screen.getByRole('radio', { name: 'Saved Searches' }));

    await waitFor(() => {
      expect(screen.getByText('Atlantic Tankers')).toBeTruthy();
    });

    await user.click(screen.getByRole('button', { name: 'Matches' }));

    expect(h.platform.savedSearchMatches).toHaveBeenCalledWith('search-tankers');

    await waitFor(() => {
      expect(screen.getByText('MT NORDIC STAR')).toBeTruthy();
    });
    expect(screen.getByText('vessels')).toBeTruthy();

    await user.click(screen.getByRole('button', { name: 'Focus Match' }));

    expect(h.runtime.stores.workspace.getState().selection).toEqual({
      feed: 'vessels',
      id: 'mmsi-211000999',
    });
  });

  it('deletes a saved search via platform.deleteSavedSearch', async () => {
    const existingSearch: SavedSearch = {
      id: 'search-b744',
      name: 'Heavy Cargo',
      query: 'B744',
      notifyOnMatch: false,
      createdAt: '2026-10-25T12:00:00Z',
    };

    const { h, user } = await setup([existingSearch]);

    await user.click(screen.getByRole('radio', { name: 'Saved Searches' }));

    await waitFor(() => {
      expect(screen.getByText('Heavy Cargo')).toBeTruthy();
    });

    await user.click(screen.getByRole('button', { name: 'Delete' }));

    expect(h.platform.deleteSavedSearch).toHaveBeenCalledWith('search-b744');

    await waitFor(() => {
      expect(screen.queryByText('Heavy Cargo')).toBeNull();
    });
  });
});
