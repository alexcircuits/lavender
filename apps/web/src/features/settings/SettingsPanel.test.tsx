import { act, fireEvent, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { type Harness, renderWithRuntime, startHarness } from '../../app/testing';
import { SettingsPanel } from './SettingsPanel';

describe('SettingsPanel', () => {
  let harness: Harness | null = null;

  afterEach(() => {
    harness?.stop();
    harness = null;
  });

  it('renders all four settings sections', async () => {
    harness = await startHarness();
    renderWithRuntime(harness.runtime, <SettingsPanel />);

    expect(screen.getByRole('heading', { name: /display & imagery/i })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: /units & coordinates/i })).toBeInTheDocument();
    expect(
      screen.getByRole('heading', { name: /data sources & credentials/i }),
    ).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: /about lavender/i })).toBeInTheDocument();
  });

  it('updates coordinate format and unit system preferences', async () => {
    harness = await startHarness();
    renderWithRuntime(harness.runtime, <SettingsPanel />);

    // Switch coordinate format to DMS
    const dmsOption = screen.getByText(/dms/i);
    fireEvent.click(dmsOption);
    expect(harness.runtime.stores.preferences.getState().coordinateFormat).toBe('dms');

    // Switch coordinate format to MGRS
    const mgrsOption = screen.getByText(/mgrs/i);
    fireEvent.click(mgrsOption);
    expect(harness.runtime.stores.preferences.getState().coordinateFormat).toBe('mgrs');

    // Switch unit system to Metric
    const metricOption = screen.getByText(/metric/i);
    fireEvent.click(metricOption);
    expect(harness.runtime.stores.preferences.getState().unitSystem).toBe('metric');

    // Switch unit system to Imperial
    const imperialOption = screen.getByText(/imperial/i);
    fireEvent.click(imperialOption);
    expect(harness.runtime.stores.preferences.getState().unitSystem).toBe('imperial');
  });

  it('updates motion and toggles Zig WASM label collision arbiter', async () => {
    harness = await startHarness();
    renderWithRuntime(harness.runtime, <SettingsPanel />);

    // Motion selection
    const reducedMotion = screen.getByText(/reduced/i);
    fireEvent.click(reducedMotion);
    expect(harness.runtime.stores.preferences.getState().motion).toBe('reduced');

    // Label arbitration toggle
    const toggle = screen.getByRole('switch', { name: /enable label collision arbiter/i });
    expect(toggle).toBeChecked();
    fireEvent.click(toggle);
    expect(harness.runtime.stores.preferences.getState().labelArbitration).toBe(false);
    expect(screen.getByText('Zig WASM SIMD')).toBeInTheDocument();

    // Tactical optics preset selection
    fireEvent.click(screen.getByText('FLIR'));
    expect(harness.runtime.stores.preferences.getState().visualPreset).toBe('thermal');
    fireEvent.click(screen.getByText('NVG'));
    expect(harness.runtime.stores.preferences.getState().visualPreset).toBe('nvg');
  });

  it('safely handles desktop provider keys without ever exposing values in the DOM', async () => {
    harness = await startHarness();
    const setSecretSpy = vi.spyOn(harness.platform, 'setSecret');

    renderWithRuntime(harness.runtime, <SettingsPanel />);

    await waitFor(() => {
      expect(screen.getByText('OS Keychain Protected')).toBeInTheDocument();
    });

    const firmsInput = screen.getByLabelText('Enter NASA FIRMS map key');
    expect(firmsInput).toHaveAttribute('type', 'password');

    const superSecretKey = 'top-secret-firms-key-abc123xyz';

    // Enter the secret key into password input
    fireEvent.change(firmsInput, { target: { value: superSecretKey } });

    // The secret string is never displayed in any text nodes
    expect(screen.queryByText(superSecretKey)).not.toBeInTheDocument();

    // Click Save
    const saveButtons = screen.getAllByRole('button', { name: /save/i });
    const firmsSaveButton = saveButtons[0];
    expect(firmsSaveButton).toBeDefined();
    if (!firmsSaveButton) throw new Error('Save button not found');

    await act(async () => {
      fireEvent.click(firmsSaveButton);
    });

    // Expect setSecret was called with exact id and secret value
    expect(setSecretSpy).toHaveBeenCalledWith('firms', superSecretKey);

    // CRITICAL SECURITY PROPERTY: Input value is wiped clean after submission
    expect(firmsInput).toHaveValue('');

    // Secret value still does not appear anywhere in DOM
    expect(screen.queryByText(superSecretKey)).not.toBeInTheDocument();
  });

  it('clears desktop provider keys on user request', async () => {
    harness = await startHarness();
    harness.platform.secretStatus = vi.fn().mockResolvedValue({
      managedBy: 'keychain',
      status: {
        firms: true,
        aisstream: false,
        opensky_id: false,
        opensky_secret: false,
        ll2: false,
        mapillary: false,
        tomtom: false,
      },
    });
    const clearSecretSpy = vi.spyOn(harness.platform, 'clearSecret');

    renderWithRuntime(harness.runtime, <SettingsPanel />);

    await waitFor(() => {
      expect(screen.getByText('Configured')).toBeInTheDocument();
    });

    const clearButton = screen.getByRole('button', { name: /clear/i });
    await act(async () => {
      fireEvent.click(clearButton);
    });

    expect(clearSecretSpy).toHaveBeenCalledWith('firms');
  });

  it('displays operator managed notice when in web mode', async () => {
    harness = await startHarness();
    harness.platform.secretStatus = vi.fn().mockResolvedValue({
      managedBy: 'operator',
      status: { firms: true },
    });

    renderWithRuntime(harness.runtime, <SettingsPanel />);

    await waitFor(() => {
      expect(screen.getByText('Server Managed')).toBeInTheDocument();
      expect(
        screen.getByText(
          /upstream data source credentials are managed securely by the server operator/i,
        ),
      ).toBeInTheDocument();
    });

    // Password input fields must NOT be rendered when managed by operator
    expect(screen.queryByLabelText('Enter NASA FIRMS map key')).not.toBeInTheDocument();
  });

  it('renders polyglot architecture overview and documentation link', async () => {
    harness = await startHarness();
    renderWithRuntime(harness.runtime, <SettingsPanel />);

    expect(screen.getByText('Rust Core')).toBeInTheDocument();
    expect(screen.getByText('Zig WASM')).toBeInTheDocument();
    expect(screen.getByText('DuckDB Analytics')).toBeInTheDocument();
    expect(screen.getByText('Mojo Perception')).toBeInTheDocument();

    const docLink = screen.getByRole('link', { name: /documentation & third-party notices/i });
    expect(docLink).toHaveAttribute('href', '/docs/');
  });
});
