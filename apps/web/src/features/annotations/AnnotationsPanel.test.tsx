import { act, fireEvent, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { type Harness, renderWithRuntime, startHarness } from '../../app/testing';
import { pin } from '../../test/annotation';
import { AnnotationsPanel } from './AnnotationsPanel';

const harnesses: Harness[] = [];
async function setup() {
  const h = await startHarness({ globe: true });
  harnesses.push(h);
  const view = renderWithRuntime(h.runtime, <AnnotationsPanel />);
  return { h, view, user: userEvent.setup() };
}
afterEach(() => {
  for (const h of harnesses.splice(0)) h.stop();
});
describe('accessible annotation editor', () => {
  it('creates with keyboard coordinate entry, edits labels/notes/geometry, and deletes from the list', async () => {
    const { h, user } = await setup();
    await user.click(screen.getByRole('button', { name: 'New pin' }));
    await user.type(screen.getByLabelText('Label', { exact: true }), 'Meeting point');
    await user.type(screen.getByLabelText('Note', { exact: true }), 'North entrance');
    await user.type(screen.getByLabelText('Latitude', { exact: true }), '60.17');
    await user.type(screen.getByLabelText('Longitude', { exact: true }), '24.94');
    await user.click(screen.getByRole('button', { name: 'Add point' }));
    screen.getByRole('button', { name: 'Save annotation' }).focus();
    await user.keyboard('{Enter}');
    expect(screen.getByRole('heading', { name: 'Meeting point' })).toBeVisible();
    expect(screen.getByText('North entrance')).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Edit Meeting point' }));
    await user.clear(screen.getByLabelText('Note', { exact: true }));
    await user.type(screen.getByLabelText('Note', { exact: true }), 'South entrance');
    await user.clear(screen.getByLabelText(/Vertices/));
    await user.type(screen.getByLabelText(/Vertices/), '60.18, 24.95');
    await user.click(screen.getByRole('button', { name: 'Apply vertices' }));
    await user.click(screen.getByRole('button', { name: 'Save annotation' }));
    expect(h.runtime.stores.workspace.getState().annotations[0]).toMatchObject({
      note: 'South entrance',
      points: [{ lat: 60.18, lon: 24.95 }],
    });
    await user.click(screen.getByRole('button', { name: 'Delete Meeting point' }));
    expect(screen.getByText(/No annotations yet/)).toBeVisible();
  });
  it('owns map clicks and supports Enter, Backspace, Escape with cleanup on unmount', async () => {
    const { h, user, view } = await setup();
    await user.click(screen.getByRole('button', { name: 'New line' }));
    act(() => {
      h.globe?.point({ lat: 0, lon: 179 });
      h.globe?.point({ lat: 0, lon: -179 });
    });
    expect(screen.getByRole('button', { name: 'Save annotation' })).toBeEnabled();
    fireEvent.keyDown(window, { key: 'Backspace' });
    expect(screen.getByRole('button', { name: 'Save annotation' })).toBeDisabled();
    act(() => h.globe?.point({ lat: 0, lon: -179 }));
    fireEvent.keyDown(window, { key: 'Enter' });
    expect(screen.getByRole('heading', { name: 'Untitled line' })).toBeVisible();
    expect(screen.getByText('Approximate 222.39 km')).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'New area' }));
    act(() => h.globe?.point({ lat: 1, lon: 1 }));
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(h.runtime.stores.workspace.getState().annotations).toHaveLength(1);
    expect(screen.queryByRole('button', { name: 'Cancel drawing' })).not.toBeInTheDocument();
    expect(h.globe?.globe.setDrawing).toHaveBeenLastCalledWith(false);
    await user.click(screen.getByRole('button', { name: 'New pin' }));
    view.unmount();
    expect(h.globe?.globe.setDrawing).toHaveBeenLastCalledWith(false);
  });
  it('reports bad coordinates and ignores duplicate clicks, while external restoration cancels a draft', async () => {
    const { h, user } = await setup();
    await user.click(screen.getByRole('button', { name: 'New line' }));
    act(() => h.globe?.point(null));
    expect(
      screen.getByRole('region', { name: 'Annotations' }).querySelector('[role=alert]'),
    ).toHaveTextContent(/Earth/);
    await user.type(screen.getByLabelText(/Vertices/), '91, 0');
    await user.click(screen.getByRole('button', { name: 'Apply vertices' }));
    expect(
      screen.getByRole('region', { name: 'Annotations' }).querySelector('[role=alert]'),
    ).toHaveTextContent(/Coordinates/);
    act(() => {
      h.globe?.point({ lat: 0, lon: 0 });
      h.globe?.point({ lat: 0, lon: 0 });
    });
    expect(
      screen.getByRole('region', { name: 'Annotations' }).querySelector('[role=alert]'),
    ).toHaveTextContent(/0.5 m/);
    act(() => {
      const state = h.runtime.stores.workspace.getState();
      state.restore(state.layers, state.imageryId, state.selection, [pin]);
    });
    expect(screen.queryByRole('button', { name: 'Cancel drawing' })).not.toBeInTheDocument();
    expect(screen.getByRole('heading', { name: pin.label })).toBeVisible();
  });
});
