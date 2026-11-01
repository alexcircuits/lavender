import { describe, expect, it, vi } from 'vitest';
import {
  createRememberingWebUsb,
  createSdrDeviceMemory,
  RTL_SDR_USB_FILTERS,
  SDR_DEVICE_STORAGE_KEY,
  type SdrStorageLike,
  type SdrUsbDeviceLike,
  type SdrWebUsbLike,
  sameSdrDevice,
  sdrDeviceIdentity,
  sdrDeviceLabel,
  selectAuthorizedSdrDevice,
} from './usbDevices';

const RTL_FILTERS = [
  { vendorId: 0x0bda, productId: 0x2832 },
  { vendorId: 0x0bda, productId: 0x2838 },
];

const flyCatcherUat: SdrUsbDeviceLike = {
  vendorId: 0x0bda,
  productId: 0x2838,
  productName: 'FlyCatcher_UAT',
  serialNumber: '00000001',
};

const flyCatcherAdsb: SdrUsbDeviceLike = {
  vendorId: 0x0bda,
  productId: 0x2838,
  productName: 'FlyCatcher_ADS_B',
  serialNumber: '00000001',
};

const genericDongle: SdrUsbDeviceLike = {
  vendorId: 0x0bda,
  productId: 0x2838,
  productName: 'RTL2838UHIDIR',
  serialNumber: '00000001',
};

const unrelated: SdrUsbDeviceLike = {
  vendorId: 0x1234,
  productId: 0x5678,
  productName: 'Other',
};

function memoryStorage(): SdrStorageLike & { values: Map<string, string> } {
  const values = new Map<string, string>();
  return {
    values,
    getItem: (key: string) => (values.has(key) ? (values.get(key) ?? null) : null),
    setItem: (key: string, value: string) => values.set(key, String(value)),
  };
}

describe('usbDevices', () => {
  it('exposes standard RTL2832U filters', () => {
    expect(RTL_SDR_USB_FILTERS).toHaveLength(2);
    expect(RTL_SDR_USB_FILTERS[0]).toEqual({ vendorId: 0x0bda, productId: 0x2832 });
    expect(RTL_SDR_USB_FILTERS[1]).toEqual({ vendorId: 0x0bda, productId: 0x2838 });
  });

  it('ADS-B mode prefers the 1090 MHz channel of a dual-channel board', () => {
    const devices = [unrelated, flyCatcherUat, flyCatcherAdsb];
    expect(selectAuthorizedSdrDevice(devices, { filters: RTL_FILTERS, mode: 'adsb' })).toBe(
      flyCatcherAdsb,
    );

    const chosen = selectAuthorizedSdrDevice(
      [genericDongle, { ...flyCatcherAdsb, productName: 'Receiver 1090' }],
      { filters: RTL_FILTERS, mode: 'adsb' },
    );
    expect(chosen?.productName).toBe('Receiver 1090');

    expect(
      selectAuthorizedSdrDevice([genericDongle], {
        filters: RTL_FILTERS,
        mode: 'adsb',
      }),
    ).toBe(genericDongle);
  });

  it('FM mode avoids ADS-B/UAT channels, and nothing matches unrelated devices', () => {
    expect(
      selectAuthorizedSdrDevice([flyCatcherAdsb, flyCatcherUat, genericDongle], {
        filters: RTL_FILTERS,
        mode: 'fm',
      }),
    ).toBe(genericDongle);

    expect(
      selectAuthorizedSdrDevice([unrelated], {
        filters: RTL_FILTERS,
        mode: 'fm',
      }),
    ).toBeNull();

    expect(selectAuthorizedSdrDevice([], { mode: 'adsb' })).toBeNull();
  });

  it('a remembered device wins, matched by the full identity rather than serial alone', () => {
    const devices = [flyCatcherAdsb, flyCatcherUat];
    expect(
      selectAuthorizedSdrDevice(devices, {
        filters: RTL_FILTERS,
        mode: 'adsb',
        remembered: { ...flyCatcherUat },
      }),
    ).toBe(flyCatcherUat);

    expect(sameSdrDevice(flyCatcherAdsb, flyCatcherUat)).toBe(false);
    expect(sameSdrDevice(flyCatcherAdsb, { ...flyCatcherAdsb })).toBe(true);

    expect(
      selectAuthorizedSdrDevice(devices, {
        filters: RTL_FILTERS,
        mode: 'adsb',
        remembered: { ...flyCatcherAdsb, serialNumber: 'unplugged' },
      }),
    ).toBe(flyCatcherAdsb);
  });

  it('device memory is per mode and tolerates unavailable storage', () => {
    const storage = memoryStorage();
    const memory = createSdrDeviceMemory(storage);
    memory.set('adsb', flyCatcherAdsb);
    memory.set('fm', genericDongle);

    expect(memory.get('adsb')).toEqual(sdrDeviceIdentity(flyCatcherAdsb));
    expect(memory.get('fm')).toEqual(sdrDeviceIdentity(genericDongle));
    expect(storage.values.get(SDR_DEVICE_STORAGE_KEY)).toContain('FlyCatcher_ADS_B');

    const blocked = createSdrDeviceMemory({
      getItem() {
        throw new Error('blocked');
      },
      setItem() {
        throw new Error('blocked');
      },
    });
    expect(blocked.get('adsb')).toBeNull();
    expect(() => blocked.set('adsb', flyCatcherAdsb)).not.toThrow();
  });

  it('the WebUSB wrapper reuses the remembered channel without the picker', async () => {
    const memory = createSdrDeviceMemory(memoryStorage());
    memory.set('adsb', flyCatcherAdsb);
    const pickerFn = vi.fn().mockResolvedValue(null);
    const selected: SdrUsbDeviceLike[] = [];

    const mockWebUsb: SdrWebUsbLike = {
      async getDevices() {
        return [flyCatcherUat, flyCatcherAdsb];
      },
      requestDevice: pickerFn,
    };

    const webUsb = createRememberingWebUsb(mockWebUsb, {
      getMode: () => 'adsb',
      memory,
      onSelected: (device) => selected.push(device),
    });

    const chosen = await webUsb.requestDevice({ filters: RTL_FILTERS });
    expect(chosen).toBe(flyCatcherAdsb);
    expect(pickerFn).not.toHaveBeenCalled();
    expect(selected).toEqual([flyCatcherAdsb]);
  });

  it('CHANGE DEVICE forces the picker once and remembers the choice for the mode', async () => {
    const memory = createSdrDeviceMemory(memoryStorage());
    memory.set('adsb', flyCatcherAdsb);
    let force = true;
    let pickerCalls = 0;

    const mockWebUsb: SdrWebUsbLike = {
      async getDevices() {
        return [flyCatcherUat, flyCatcherAdsb];
      },
      async requestDevice() {
        pickerCalls += 1;
        return flyCatcherUat;
      },
    };

    const webUsb = createRememberingWebUsb(mockWebUsb, {
      getMode: () => 'adsb',
      memory,
      consumeForcePicker: () => {
        const value = force;
        force = false;
        return value;
      },
    });

    expect(await webUsb.requestDevice({ filters: RTL_FILTERS })).toBe(flyCatcherUat);
    expect(pickerCalls).toBe(1);
    expect(memory.get('adsb')).toEqual(sdrDeviceIdentity(flyCatcherUat));

    expect(await webUsb.requestDevice({ filters: RTL_FILTERS })).toBe(flyCatcherUat);
    expect(pickerCalls).toBe(1);
  });

  it('automatic selection is not remembered, so a fallback never becomes sticky', async () => {
    const memory = createSdrDeviceMemory(memoryStorage());
    const mockWebUsb: SdrWebUsbLike = {
      async getDevices() {
        return [flyCatcherUat];
      },
      async requestDevice() {
        throw new Error('picker should not open');
      },
    };

    const webUsb = createRememberingWebUsb(mockWebUsb, {
      getMode: () => 'adsb',
      memory,
    });

    expect(await webUsb.requestDevice({ filters: RTL_FILTERS })).toBe(flyCatcherUat);
    expect(memory.get('adsb')).toBeNull();
  });

  it('returns human-readable device labels', () => {
    expect(sdrDeviceLabel(flyCatcherAdsb)).toBe('FlyCatcher_ADS_B');
    expect(sdrDeviceLabel(null)).toBe('RTL-SDR');
    expect(sdrDeviceLabel({})).toBe('RTL-SDR');
    expect(sdrDeviceLabel({ vendorId: 0x0bda })).toBe('RTL-SDR');
  });
});
