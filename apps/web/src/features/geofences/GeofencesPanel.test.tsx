import { act, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { type Harness, renderWithRuntime, startHarness } from '../../app/testing';
import type { GeoPoint } from '../../contracts';
import { GLOBE_REGION_ID } from '../commands';
import { GeofencesPanel } from './GeofencesPanel';

const harnesses: Harness[] = [];

/** Renders the panel attached to the recording globe; `click` emits a map point. */
async function setup() {
  const h = await startHarness({ globe: true });
  harnesses.push(h);
  const view = renderWithRuntime(
    h.runtime,
    <>
      <main id={GLOBE_REGION_ID} tabIndex={-1} aria-label="Globe" />
      <GeofencesPanel />
    </>,
  );
  const click = (p: GeoPoint) =>
    act(() => {
      h.globe?.point(p);
    });
  return { h, view, user: userEvent.setup(), click };
}

afterEach(() => {
  for (const h of harnesses.splice(0)) h.stop();
});

describe('geofence drawing panel', () => {
  it('draws a box from two clicks, names it, and persists it as a geofence annotation', async () => {
    const { h, user, click } = await setup();
    await user.click(screen.getByRole('button', { name: 'Draw box' }));
    click({ lat: 45, lon: -73 });
    click({ lat: 41, lon: -70 });
    // Two clicks close the box; the naming form appears.
    expect(screen.getByText('Name this geofence')).toBeTruthy();
    await user.type(screen.getByLabelText('Name'), 'Approach box');
    await user.click(screen.getByRole('button', { name: 'Save geofence' }));
    const annotations = h.runtime.stores.workspace.getState().annotations;
    expect(annotations).toHaveLength(1);
    expect(annotations[0]?.kind).toBe('geofence');
    expect(annotations[0]?.label).toBe('Approach box');
    expect(annotations[0]?.points).toHaveLength(4);
    // The saved fence is listed with its actions.
    expect(screen.getByText('Approach box')).toBeTruthy();
  });

  it('hides and deletes a saved fence for this session', async () => {
    const { h, user, click } = await setup();
    await user.click(screen.getByRole('button', { name: 'Draw box' }));
    click({ lat: 10, lon: 10 });
    click({ lat: 12, lon: 12 });
    await user.type(screen.getByLabelText('Name'), 'Sector A');
    await user.click(screen.getByRole('button', { name: 'Save geofence' }));
    const id = h.runtime.stores.workspace.getState().annotations[0]?.id as string;
    await user.click(screen.getByRole('button', { name: 'Hide' }));
    expect(h.runtime.stores.workspace.getState().hiddenGeofences.has(id)).toBe(true);
    await user.click(screen.getByRole('button', { name: 'Show' }));
    expect(h.runtime.stores.workspace.getState().hiddenGeofences.has(id)).toBe(false);
    await user.click(screen.getByRole('button', { name: 'Delete' }));
    expect(h.runtime.stores.workspace.getState().annotations).toHaveLength(0);
  });

  it('keeps invalid input honest: a one-vertex polygon cannot close', async () => {
    const { user, click } = await setup();
    await user.click(screen.getByRole('button', { name: 'Draw polygon' }));
    click({ lat: 0, lon: 0 });
    expect(screen.getByRole('button', { name: 'Close shape' })).toHaveProperty('disabled', true);
    expect(screen.getByText(/1 vertices/)).toBeTruthy();
  });
});
