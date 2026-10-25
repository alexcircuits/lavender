import { act, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { type Harness, renderWithRuntime, startHarness } from '../../app/testing';
import type { MapAnnotation } from '../annotations/model';
import { AlertsPanel } from './AlertsPanel';
import { useAlertsStore } from './alertsStore';

const harnesses: Harness[] = [];

async function setup() {
  const h = await startHarness({ globe: true });
  harnesses.push(h);

  const sampleFence: MapAnnotation = {
    id: 'fence-1',
    kind: 'geofence',
    label: 'Restricted Airspace Alpha',
    note: 'Monitoring perimeter',
    createdAt: '2026-10-25T12:00:00.000Z',
    approximate: true,
    points: [
      { lat: 10, lon: 10 },
      { lat: 10, lon: 20 },
      { lat: 20, lon: 20 },
      { lat: 20, lon: 10 },
    ],
  };

  h.runtime.stores.workspace.getState().setAnnotations([sampleFence]);

  const view = renderWithRuntime(h.runtime, <AlertsPanel />);
  return { h, view, user: userEvent.setup(), sampleFence };
}

describe('AlertsPanel (Alert Inbox)', () => {
  beforeEach(() => {
    useAlertsStore.getState().clearAll();
  });

  afterEach(() => {
    for (const h of harnesses.splice(0)) {
      h.stop();
    }
    useAlertsStore.getState().clearAll();
  });

  it('renders empty state when there are no alerts', async () => {
    await setup();
    expect(screen.getByText('Alert Inbox')).toBeTruthy();
    expect(screen.getByText('No alerts')).toBeTruthy();
    expect(screen.getByText('No geofence alert events recorded.')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Clear All' })).toHaveProperty('disabled', true);
  });

  it('renders alerts with badges, entity label, and timestamps', async () => {
    await setup();

    act(() => {
      useAlertsStore.getState().addAlert({
        id: 'alert-1',
        timestamp: Date.parse('2026-10-25T14:30:00.000Z'),
        type: 'enter',
        entity: {
          feed: 'aircraft',
          id: 'plane-101',
          label: 'AF102 Boeing 787',
        },
        geofenceId: 'fence-1',
        geofenceName: 'Restricted Airspace Alpha',
        position: { lat: 15.5, lon: 15.2 },
      });
    });

    expect(screen.getByText('ENTER')).toBeTruthy();
    expect(screen.getByText('aircraft')).toBeTruthy();
    expect(screen.getByText('Restricted Airspace Alpha')).toBeTruthy();
    expect(screen.getByText('AF102 Boeing 787')).toBeTruthy();
    expect(screen.getByText(/15\.5000°,\s*15\.2000°/)).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Focus Entity' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Focus Geofence' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Dismiss' })).toBeTruthy();
  });

  it('focuses the entity and selects it in the workspace store', async () => {
    const { h, user } = await setup();

    act(() => {
      useAlertsStore.getState().addAlert({
        id: 'alert-1',
        timestamp: Date.now(),
        type: 'enter',
        entity: {
          feed: 'aircraft',
          id: 'plane-101',
          label: 'AF102',
        },
        geofenceId: 'fence-1',
        geofenceName: 'Restricted Airspace Alpha',
        position: { lat: 15.5, lon: 15.2 },
      });
    });

    await user.click(screen.getByRole('button', { name: 'Focus Entity' }));

    const selection = h.runtime.stores.workspace.getState().selection;
    expect(selection).toEqual({ feed: 'aircraft', id: 'plane-101' });

    expect(h.globe?.globe.flyTo).toHaveBeenCalledWith({
      kind: 'point',
      position: { lat: 15.5, lon: 15.2 },
    });
  });

  it('focuses the geofence centre on the globe', async () => {
    const { h, user } = await setup();

    act(() => {
      useAlertsStore.getState().addAlert({
        id: 'alert-1',
        timestamp: Date.now(),
        type: 'exit',
        entity: {
          feed: 'vessels',
          id: 'vessel-55',
          label: 'Atlantic Express',
        },
        geofenceId: 'fence-1',
        geofenceName: 'Restricted Airspace Alpha',
        position: { lat: 5.0, lon: 5.0 },
      });
    });

    await user.click(screen.getByRole('button', { name: 'Focus Geofence' }));

    expect(h.globe?.globe.flyTo).toHaveBeenCalled();
    const callArg = vi.mocked(h.globe?.globe.flyTo)?.mock.calls.at(-1)?.[0];
    expect(callArg?.kind).toBe('point');
    if (callArg?.kind === 'point') {
      expect(callArg.position.lat).toBeCloseTo(15, 0);
      expect(callArg.position.lon).toBeCloseTo(15, 0);
    }
  });

  it('dismisses an individual alert and decrements unread count', async () => {
    const { user } = await setup();

    act(() => {
      useAlertsStore.getState().addAlert({
        id: 'alert-1',
        timestamp: Date.now(),
        type: 'enter',
        entity: {
          feed: 'aircraft',
          id: 'plane-101',
        },
        geofenceId: 'fence-1',
        geofenceName: 'Restricted Airspace Alpha',
        position: { lat: 12, lon: 12 },
      });
    });

    expect(useAlertsStore.getState().unreadCount).toBe(1);
    await user.click(screen.getByRole('button', { name: 'Dismiss' }));

    expect(useAlertsStore.getState().alerts[0]?.dismissed).toBe(true);
    expect(useAlertsStore.getState().unreadCount).toBe(0);
    expect(screen.queryByRole('button', { name: 'Dismiss' })).toBeNull();
    expect(screen.getByRole('button', { name: 'Dismissed' })).toHaveProperty('disabled', true);
  });

  it('clears all alerts when Clear All is clicked', async () => {
    const { user } = await setup();

    act(() => {
      useAlertsStore.getState().addAlert({
        id: 'alert-1',
        timestamp: Date.now(),
        type: 'enter',
        entity: { feed: 'aircraft', id: 'plane-1' },
        geofenceId: 'fence-1',
        geofenceName: 'Fence 1',
        position: { lat: 10, lon: 10 },
      });
      useAlertsStore.getState().addAlert({
        id: 'alert-2',
        timestamp: Date.now(),
        type: 'exit',
        entity: { feed: 'vessels', id: 'vessel-2' },
        geofenceId: 'fence-1',
        geofenceName: 'Fence 1',
        position: { lat: 2, lon: 2 },
      });
    });

    expect(useAlertsStore.getState().alerts).toHaveLength(2);
    expect(screen.getByRole('button', { name: 'Clear All' })).toHaveProperty('disabled', false);

    await user.click(screen.getByRole('button', { name: 'Clear All' }));

    expect(useAlertsStore.getState().alerts).toHaveLength(0);
    expect(useAlertsStore.getState().unreadCount).toBe(0);
    expect(screen.getByText('No alerts')).toBeTruthy();
  });

  it('closes the panel when the close button is clicked', async () => {
    const { h, user } = await setup();
    h.runtime.stores.ui.getState().setSidePanel('alerts');

    await user.click(screen.getByRole('button', { name: 'Close alerts' }));

    expect(h.runtime.stores.ui.getState().sidePanel).toBeNull();
  });
});
