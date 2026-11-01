import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { LocalSdrPanel } from './LocalSdrPanel';
import { SdrController } from './sdrController';
import type { SdrStorageLike, SdrUsbDeviceLike, SdrWebUsbLike } from './usbDevices';

function parseHex(hex: string): Uint8Array {
  const clean = hex.replace(/[\s\r\n]+/g, '');
  const bytes = new Uint8Array(clean.length / 2);
  for (let i = 0; i < bytes.length; i++) {
    bytes[i] = Number.parseInt(clean.slice(i * 2, i * 2 + 2), 16);
  }
  return bytes;
}

function synthesizeIq(
  bytes: Uint8Array,
  {
    floor = 138,
    high = 148,
    preambleTail = 145,
  }: { floor?: number; high?: number; preambleTail?: number | null } = {},
): ArrayBuffer {
  const sampleCount = 16 + 112 * 2 + 8;
  const iq = new Uint8Array(sampleCount * 2).fill(128);
  for (let sample = 0; sample < sampleCount; sample += 1) {
    iq[sample * 2] = floor;
  }
  const pulse = (sample: number) => {
    iq[sample * 2] = high;
  };
  for (const offset of [0, 2, 7, 9]) pulse(offset);
  if (preambleTail !== null && Number.isFinite(preambleTail)) {
    for (let sample = 10; sample < 16; sample += 1) {
      iq[sample * 2] = preambleTail;
    }
  }
  for (let index = 0; index < 112; index += 1) {
    const value = ((bytes[index >> 3] ?? 0) >> (7 - (index & 7))) & 1;
    pulse(16 + index * 2 + (value ? 0 : 1));
  }
  return iq.buffer;
}

const mockDevice: SdrUsbDeviceLike = {
  vendorId: 0x0bda,
  productId: 0x2838,
  productName: 'RTL2838UHIDIR',
  serialNumber: '00000001',
  open: vi.fn().mockResolvedValue(undefined),
  close: vi.fn().mockResolvedValue(undefined),
  selectConfiguration: vi.fn().mockResolvedValue(undefined),
  claimInterface: vi.fn().mockResolvedValue(undefined),
};

function createMockWebUsb(device: SdrUsbDeviceLike = mockDevice): SdrWebUsbLike {
  return {
    getDevices: vi.fn().mockResolvedValue([device]),
    requestDevice: vi.fn().mockResolvedValue(device),
  };
}

function createMockStorage(): SdrStorageLike {
  const store = new Map<string, string>();
  return {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => store.set(key, value),
  };
}

afterEach(() => {
  cleanup();
});

describe('LocalSdrPanel', () => {
  it('renders unsupported warning when WebUSB is not present', () => {
    const controller = new SdrController({ webUsb: null, storage: createMockStorage() });
    render(<LocalSdrPanel controller={controller} />);

    expect(screen.getAllByText(/WebUSB is not supported in this browser/i).length).toBeGreaterThan(
      0,
    );
    expect(screen.getByText('Unsupported')).toBeInTheDocument();
    const btn = screen.getByRole('button', { name: /Connect RTL-SDR/i });
    expect(btn).toBeDisabled();
  });

  it('renders initial idle state with connect button when WebUSB is supported', () => {
    const mockWebUsb = createMockWebUsb();
    const controller = new SdrController({ webUsb: mockWebUsb, storage: createMockStorage() });
    render(<LocalSdrPanel controller={controller} />);

    expect(screen.getByText('Disconnected')).toBeInTheDocument();
    const btn = screen.getByRole('button', { name: /Connect RTL-SDR/i });
    expect(btn).toBeEnabled();
    expect(screen.queryByText(/WebUSB is not supported/i)).not.toBeInTheDocument();
  });

  it('connects to RTL-SDR device and renders connected state', async () => {
    const mockWebUsb = createMockWebUsb();
    const controller = new SdrController({ webUsb: mockWebUsb, storage: createMockStorage() });
    render(<LocalSdrPanel controller={controller} />);

    const connectBtn = screen.getByRole('button', { name: /Connect RTL-SDR/i });
    fireEvent.click(connectBtn);

    // Wait for state update
    expect(await screen.findByText('Connected')).toBeInTheDocument();
    expect(screen.getByText('RTL2838UHIDIR')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Disconnect/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Change Device/i })).toBeInTheDocument();
  });

  it('switches receiver modes between ADS-B and FM', async () => {
    const mockWebUsb = createMockWebUsb();
    const controller = new SdrController({ webUsb: mockWebUsb, storage: createMockStorage() });
    render(<LocalSdrPanel controller={controller} />);

    // Initial mode is ADS-B 1090 MHz
    const freqInput = screen.getByLabelText(/Tuned center frequency/i) as HTMLInputElement;
    expect(freqInput.value).toBe('1090.000');

    // Switch to FM
    const fmRadio = screen.getByRole('radio', { name: /FM \(87\.5–108 MHz\)/i });
    fireEvent.click(fmRadio);

    expect(controller.getState().mode).toBe('fm');
    expect(freqInput.value).toBe('98.500');

    // Switch back to ADS-B
    const adsbRadio = screen.getByRole('radio', { name: /ADS-B \(1090 MHz\)/i });
    fireEvent.click(adsbRadio);

    expect(controller.getState().mode).toBe('adsb');
    expect(freqInput.value).toBe('1090.000');
  });

  it('allows tuning the center frequency input', async () => {
    const mockWebUsb = createMockWebUsb();
    const controller = new SdrController({ webUsb: mockWebUsb, storage: createMockStorage() });
    render(<LocalSdrPanel controller={controller} />);

    const freqInput = screen.getByLabelText(/Tuned center frequency/i) as HTMLInputElement;
    fireEvent.change(freqInput, { target: { value: '1090.500' } });
    fireEvent.blur(freqInput);

    expect(controller.getState().frequencyHz).toBe(1_090_500_000);
    expect(freqInput.value).toBe('1090.500');

    // Invalid input resets on blur
    fireEvent.change(freqInput, { target: { value: 'invalid' } });
    fireEvent.blur(freqInput);
    expect(freqInput.value).toBe('1090.500');
  });

  it('updates tuner gain setting and persists it', async () => {
    const storage = createMockStorage();
    const mockWebUsb = createMockWebUsb();
    const controller = new SdrController({ webUsb: mockWebUsb, storage });
    render(<LocalSdrPanel controller={controller} />);

    const gainSelect = screen.getByLabelText(/Tuner gain setting/i) as HTMLSelectElement;
    expect(gainSelect.value).toBe('28'); // ADS-B default is 28.0 dB

    fireEvent.change(gainSelect, { target: { value: '33.8' } });
    expect(controller.getState().gain).toBe(33.8);
    expect(gainSelect.value).toBe('33.8');

    fireEvent.change(gainSelect, { target: { value: 'auto' } });
    expect(controller.getState().gain).toBe('auto');
    expect(gainSelect.value).toBe('auto');
  });

  it('displays live telemetry stats when packets are fed', async () => {
    const mockWebUsb = createMockWebUsb();
    const controller = new SdrController({ webUsb: mockWebUsb, storage: createMockStorage() });
    render(<LocalSdrPanel controller={controller} />);

    expect(screen.getByTestId('sdr-stat-rate')).toHaveTextContent('0.0 msg/s');
    expect(screen.getByTestId('sdr-stat-aircraft')).toHaveTextContent('0');
    expect(screen.getByTestId('sdr-stat-frames')).toHaveTextContent('0');
    expect(screen.getByTestId('sdr-stat-buffer')).toHaveTextContent('IDLE');

    // Feed a valid Mode-S frame (KL1023 aircraft ICAO 4840D6)
    const validFrame = parseHex('8D4840D6202CC371C32CE0576098');
    const iqData = synthesizeIq(validFrame);
    controller.feedIqData(iqData, 2_000_000);

    await waitFor(() => {
      expect(screen.getByTestId('sdr-stat-aircraft')).toHaveTextContent('1');
    });
    expect(screen.getByTestId('sdr-stat-frames')).toHaveTextContent('1');
    expect(screen.getByTestId('sdr-stat-rate')).toHaveTextContent('1.0 msg/s');
    expect(screen.getByTestId('sdr-stat-buffer')).toHaveTextContent('OK');
  });

  it('handles disconnect button click', async () => {
    const mockWebUsb = createMockWebUsb();
    const controller = new SdrController({ webUsb: mockWebUsb, storage: createMockStorage() });
    await controller.connect();

    render(<LocalSdrPanel controller={controller} />);
    expect(screen.getByText('Connected')).toBeInTheDocument();

    const disconnectBtn = screen.getByRole('button', { name: /Disconnect/i });
    fireEvent.click(disconnectBtn);

    expect(await screen.findByText('Disconnected')).toBeInTheDocument();
    expect(controller.getState().connected).toBe(false);
  });
});
