import { describe, expect, it, vi } from 'vitest';
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

function createMockStorage(): SdrStorageLike {
  const store = new Map<string, string>();
  return {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => store.set(key, value),
  };
}

describe('SdrController', () => {
  it('initializes with unsupported status when WebUSB is missing', () => {
    const controller = new SdrController({ webUsb: null, storage: createMockStorage() });
    const state = controller.getState();
    expect(state.webUsbSupported).toBe(false);
    expect(state.status).toBe('unsupported');
    expect(state.connected).toBe(false);
    expect(state.mode).toBe('adsb');
    expect(state.frequencyHz).toBe(1_090_000_000);
    expect(state.sampleRate).toBe(2_000_000);
  });

  it('connects to WebUSB device and opens interface', async () => {
    const mockDevice: SdrUsbDeviceLike = {
      vendorId: 0x0bda,
      productId: 0x2838,
      productName: 'RTL2838UHIDIR',
      open: vi.fn().mockResolvedValue(undefined),
      selectConfiguration: vi.fn().mockResolvedValue(undefined),
      claimInterface: vi.fn().mockResolvedValue(undefined),
      close: vi.fn().mockResolvedValue(undefined),
    };

    const mockWebUsb: SdrWebUsbLike = {
      getDevices: vi.fn().mockResolvedValue([mockDevice]),
      requestDevice: vi.fn().mockResolvedValue(mockDevice),
    };

    const controller = new SdrController({ webUsb: mockWebUsb, storage: createMockStorage() });
    const success = await controller.connect('adsb');

    expect(success).toBe(true);
    expect(mockDevice.open).toHaveBeenCalled();
    expect(mockDevice.selectConfiguration).toHaveBeenCalledWith(1);
    expect(mockDevice.claimInterface).toHaveBeenCalledWith(0);

    const state = controller.getState();
    expect(state.connected).toBe(true);
    expect(state.status).toBe('connected');
    expect(state.deviceLabel).toBe('RTL2838UHIDIR');

    // Disconnect cleans up
    await controller.disconnect();
    expect(mockDevice.close).toHaveBeenCalled();
    expect(controller.getState().connected).toBe(false);
    expect(controller.getState().status).toBe('idle');
  });

  it('handles user cancellation during device request', async () => {
    const mockWebUsb: SdrWebUsbLike = {
      getDevices: vi.fn().mockResolvedValue([]),
      requestDevice: vi
        .fn()
        .mockRejectedValue(new Error('User cancelled the requestDevice() chooser.')),
    };

    const controller = new SdrController({ webUsb: mockWebUsb, storage: createMockStorage() });
    const success = await controller.connect();

    expect(success).toBe(false);
    const state = controller.getState();
    expect(state.connected).toBe(false);
    expect(state.status).toBe('idle');
    expect(state.message).toContain('cancelled');
  });

  it('handles device errors during connect', async () => {
    const mockWebUsb: SdrWebUsbLike = {
      getDevices: vi.fn().mockResolvedValue([]),
      requestDevice: vi.fn().mockRejectedValue(new Error('Device already claimed by kernel')),
    };

    const controller = new SdrController({ webUsb: mockWebUsb, storage: createMockStorage() });
    const success = await controller.connect();

    expect(success).toBe(false);
    const state = controller.getState();
    expect(state.connected).toBe(false);
    expect(state.status).toBe('error');
    expect(state.message).toContain('Device already claimed');
  });

  it('switches modes and updates presets', async () => {
    const controller = new SdrController({ webUsb: null, storage: createMockStorage() });
    await controller.setMode('fm');

    expect(controller.getState().mode).toBe('fm');
    expect(controller.getState().frequencyHz).toBe(98_500_000);
    expect(controller.getState().sampleRate).toBe(2_048_000);
    expect(controller.getState().gain).toBe('auto');

    await controller.setMode('adsb');
    expect(controller.getState().mode).toBe('adsb');
    expect(controller.getState().frequencyHz).toBe(1_090_000_000);
    expect(controller.getState().sampleRate).toBe(2_000_000);
    expect(controller.getState().gain).toBe(28.0);
  });

  it('tunes frequency and validates input', async () => {
    const controller = new SdrController({ webUsb: null, storage: createMockStorage() });
    await controller.setFrequency(99_100_000);
    expect(controller.getState().frequencyHz).toBe(99_100_000);

    // Rejects non-positive or non-finite values
    await controller.setFrequency(-100);
    expect(controller.getState().frequencyHz).toBe(99_100_000);
    await controller.setFrequency(Number.NaN);
    expect(controller.getState().frequencyHz).toBe(99_100_000);
  });

  it('sets and normalizes tuner gain', async () => {
    const storage = createMockStorage();
    const controller = new SdrController({ webUsb: null, storage });

    await controller.setGain(30);
    expect(controller.getState().gain).toBe(29.7); // Nearest R820T step

    await controller.setGain('auto');
    expect(controller.getState().gain).toBe('auto');
  });

  it('decodes ADS-B packets and tracks aircraft', () => {
    const controller = new SdrController({ webUsb: null, storage: createMockStorage() });
    const validFrame = parseHex('8D4840D6202CC371C32CE0576098');
    const iqData = synthesizeIq(validFrame);

    const frames = controller.feedIqData(iqData, 2_000_000);
    expect(frames).toHaveLength(1);

    const state = controller.getState();
    expect(state.totalFrames).toBe(1);
    expect(state.aircraftCount).toBe(1);
    expect(state.messageRate).toBe(1);
    expect(state.bufferStatus).toBe('ok');
    expect(state.iqBytesReceived).toBe(iqData.byteLength);

    const aircraft = controller.getTrackedAircraft();
    expect(aircraft).toHaveLength(1);
    expect(aircraft[0]?.icao).toBe('4840D6');

    // Reset stats clears tracking
    controller.resetStats();
    expect(controller.getState().totalFrames).toBe(0);
    expect(controller.getState().aircraftCount).toBe(0);
    expect(controller.getTrackedAircraft()).toHaveLength(0);
  });
});
