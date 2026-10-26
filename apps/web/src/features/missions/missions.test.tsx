import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { type Harness, renderWithRuntime, startHarness } from '../../app/testing';
import { memoryPreferences } from '../../test/fakes';
import { MissionsPanel } from './MissionsPanel';
import { MISSIONS_PREF_KEY, validateMissions } from './model';

const harnesses: Harness[] = [];

async function setup() {
  const h = await startHarness();
  harnesses.push(h);
  const view = renderWithRuntime(h.runtime, <MissionsPanel />);
  return { h, view, user: userEvent.setup() };
}

afterEach(() => {
  for (const h of harnesses.splice(0)) h.stop();
});

describe('mission board', () => {
  it('adds a card to Detected, moves it along the workflow and deletes it', async () => {
    const { h, user } = await setup();
    const detected = screen.getByLabelText('Detected');
    const titleInput = screen.getByLabelText('New card in Detected');
    await user.type(titleInput, 'Odd transponder near fence');
    await user.click(within(detected).getByRole('button', { name: 'Add card' }));
    expect(screen.getByText('Odd transponder near fence')).toBeTruthy();
    expect(detected).toHaveTextContent('Detected · 1');

    await user.click(screen.getByRole('button', { name: 'To Watching' }));
    expect(screen.getByLabelText('Watching')).toHaveTextContent('Watching · 1');
    expect(screen.getByLabelText('Detected')).toHaveTextContent('Detected · 0');

    await user.click(screen.getByRole('button', { name: 'To Monitoring' }));
    expect(screen.getByLabelText('Monitoring')).toHaveTextContent('Monitoring · 1');

    await user.click(screen.getByRole('button', { name: 'Delete' }));
    expect(screen.getByLabelText('Monitoring')).toHaveTextContent('Monitoring · 0');
    expect(h.runtime.stores.workspace.getState().annotations).toEqual([]);
  });

  it('persists across a reload and rejects malformed saved boards', async () => {
    const first = await startHarness();
    harnesses.push(first);
    first.platform.preferences = memoryPreferences();
    const view1 = renderWithRuntime(first.runtime, <MissionsPanel />);
    await userEvent.type(screen.getByLabelText('New card in Detected'), 'Persisted card');
    await userEvent.click(
      within(screen.getByLabelText('Detected')).getByRole('button', { name: 'Add card' }),
    );
    view1.unmount();

    // A fresh mount hydrates from the same preference store.
    const view2 = renderWithRuntime(first.runtime, <MissionsPanel />);
    expect(await screen.findByText('Persisted card')).toBeTruthy();
    view2.unmount();

    // Corrupted persisted data is reported, never rendered.
    first.platform.preferences = memoryPreferences({
      [MISSIONS_PREF_KEY]: [{ id: 'x', title: '', note: '', column: 'nonsense' }],
    });
    const view3 = renderWithRuntime(first.runtime, <MissionsPanel />);
    expect(await screen.findByText(/Invalid card|could not be read/)).toBeTruthy();
    view3.unmount();
  });

  it('links the card to the entity selected at add time', async () => {
    const { h, user } = await setup();
    h.runtime.stores.workspace.getState().select({ feed: 'aircraft', id: 'abc123' });
    await user.type(screen.getByLabelText('New card in Detected'), 'Track this');
    await user.click(
      within(screen.getByLabelText('Detected')).getByRole('button', { name: 'Add card' }),
    );
    expect(screen.getByText(/Entity: aircraft:abc123/)).toBeTruthy();

    h.runtime.stores.workspace.getState().select(null);
    await user.click(screen.getByRole('button', { name: 'Focus entity' }));
    expect(h.runtime.stores.workspace.getState().selection).toEqual({
      feed: 'aircraft',
      id: 'abc123',
    });
  });

  it('saves and loads composite workspace scene snapshots', async () => {
    const { h, user } = await setup();
    const ws = h.runtime.stores.workspace;
    ws.getState().setLayerEnabled('aircraft', true);
    ws.getState().setImagery('night');
    ws.getState().select({ feed: 'aircraft', id: '4aca2d' });

    await user.type(screen.getByLabelText('Scene snapshot name'), 'Baltic Patrol');
    await user.click(screen.getByRole('button', { name: 'Save current scene' }));
    expect(screen.getByRole('heading', { name: 'Baltic Patrol' })).toBeVisible();

    ws.getState().setLayerEnabled('aircraft', false);
    ws.getState().setImagery('default');
    ws.getState().select(null);

    await user.click(screen.getByRole('button', { name: 'Load Baltic Patrol' }));
    expect(ws.getState().layers.aircraft?.enabled).toBe(true);
    expect(ws.getState().imageryId).toBe('night');
    expect(ws.getState().selection).toEqual({ feed: 'aircraft', id: '4aca2d' });

    await user.click(screen.getByRole('button', { name: 'Remove Baltic Patrol' }));
    expect(screen.queryByRole('heading', { name: 'Baltic Patrol' })).not.toBeInTheDocument();
  });
});

describe('mission model', () => {
  it('round-trips valid cards and refuses junk', () => {
    const card = {
      id: 'card-1',
      title: 'Title',
      note: '',
      column: 'watching' as const,
      createdAt: '2026-10-24T00:00:00Z',
      entity: { feed: 'vessels', id: '123' },
      geofenceId: null,
    };
    expect(validateMissions([card])).toEqual([card]);
    expect(() => validateMissions({})).toThrow();
    expect(() => validateMissions([{ ...card, column: 'later' }])).toThrow();
    expect(() => validateMissions([{ ...card, id: 'bad id!' }])).toThrow();
    expect(() => validateMissions([{ ...card, entity: { feed: 3 } }])).toThrow();
  });
});
