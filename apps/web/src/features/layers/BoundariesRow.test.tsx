import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { type Harness, renderWithRuntime, startHarness } from '../../app/testing';
import { catalogOf } from '../../test/fakes';
import { BoundariesRow } from './BoundariesRow';

let harness: Harness | null = null;

async function setup() {
  harness = await startHarness({ catalog: catalogOf([]), globe: true });
  const user = userEvent.setup();
  const view = renderWithRuntime(
    harness.runtime,
    <ul>
      <BoundariesRow />
    </ul>,
  );
  return { ...harness, user, view };
}

afterEach(() => {
  harness?.stop();
  harness = null;
  vi.restoreAllMocks();
});

describe('BoundariesRow', () => {
  it('renders Boundaries row with title and public domain source attribution', async () => {
    await setup();
    expect(screen.getByText('Boundaries')).toBeInTheDocument();
    expect(screen.getByText('Natural Earth · Public domain')).toBeInTheDocument();
    expect(screen.getByRole('switch', { name: 'Boundaries' })).toBeChecked();
  });

  it('toggles master boundaries switch and updates globe visibility', async () => {
    const { user, runtime, globe } = await setup();
    const masterSwitch = screen.getByRole('switch', { name: 'Boundaries' });
    expect(masterSwitch).toBeChecked();

    await user.click(masterSwitch);
    expect(runtime.stores.workspace.getState().boundariesEnabled).toBe(false);
    expect(globe?.globe.setBoundaryClassVisibility).toHaveBeenCalledWith({
      internationalLand: false,
      disputedLine: false,
      breakawayArea: false,
      maritimeIndicative: false,
    });

    await user.click(masterSwitch);
    expect(runtime.stores.workspace.getState().boundariesEnabled).toBe(true);
    expect(globe?.globe.setBoundaryClassVisibility).toHaveBeenCalledWith({
      internationalLand: true,
      disputedLine: true,
      breakawayArea: true,
      maritimeIndicative: true,
    });
  });

  it('expands options disclosure and toggles individual boundary classes', async () => {
    const { user, runtime, globe } = await setup();

    // Click options button to open disclosure
    const optionsButton = screen.getByRole('button', { name: 'Boundaries options' });
    await user.click(optionsButton);

    // Verify all 4 class toggles are present
    const disputedSwitch = screen.getByRole('switch', { name: 'Disputed boundary lines' });
    const breakawaySwitch = screen.getByRole('switch', { name: 'Breakaway & contested areas' });
    const maritimeSwitch = screen.getByRole('switch', { name: 'Maritime indicative lines' });
    const internationalSwitch = screen.getByRole('switch', { name: 'International land borders' });

    expect(disputedSwitch).toBeChecked();
    expect(breakawaySwitch).toBeChecked();
    expect(maritimeSwitch).toBeChecked();
    expect(internationalSwitch).toBeChecked();

    // Verify point-of-view disclaimer and attribution are plainly shown
    expect(
      screen.getByText(
        'Natural Earth shows de facto boundaries; disputed-area classification represents one point of view.',
      ),
    ).toBeInTheDocument();
    expect(screen.getByText('Made with Natural Earth (public domain)')).toBeInTheDocument();

    // Toggle disputed boundary lines off
    await user.click(disputedSwitch);
    expect(runtime.stores.workspace.getState().boundaryClasses.disputedLine).toBe(false);
    expect(globe?.globe.setBoundaryClassVisibility).toHaveBeenCalledWith(
      expect.objectContaining({ disputedLine: false }),
    );

    // Toggle breakaway areas off
    await user.click(breakawaySwitch);
    expect(runtime.stores.workspace.getState().boundaryClasses.breakawayArea).toBe(false);
    expect(globe?.globe.setBoundaryClassVisibility).toHaveBeenCalledWith(
      expect.objectContaining({ breakawayArea: false }),
    );
  });
});
