/**
 * WebUSB receiver selection for the local RTL-SDR.
 *
 * Dual-channel boards such as the Nooelec FlyCatcher enumerate as two USB
 * devices (an ADS-B 1090 MHz channel and a UAT 978 MHz channel) that can share
 * a serial number. Selection therefore remembers the full identity
 * (vendorId, productId, productName, serialNumber) per receiver mode and, for
 * ADS-B, prefers a channel whose product name names ADS-B or 1090 MHz.
 */

export const SDR_DEVICE_STORAGE_KEY = 'gev:sdr:device:v1';

export type SdrReceiverMode = 'adsb' | 'fm';

export interface SdrDeviceIdentity {
  readonly vendorId: number | null;
  readonly productId: number | null;
  readonly productName: string | null;
  readonly serialNumber: string | null;
}

export interface SdrUsbFilter {
  readonly vendorId?: number | undefined;
  readonly productId?: number | undefined;
  readonly serialNumber?: string | undefined;
}

export interface SdrDeviceMemory {
  get(mode: SdrReceiverMode | string): SdrDeviceIdentity | null;
  set(mode: SdrReceiverMode | string, device: unknown): void;
}

export interface SdrUsbDeviceLike {
  readonly vendorId?: number | undefined;
  readonly productId?: number | undefined;
  readonly productName?: string | null | undefined;
  readonly serialNumber?: string | null | undefined;
  open?(): Promise<void>;
  close?(): Promise<void>;
  selectConfiguration?(configurationValue: number): Promise<void>;
  claimInterface?(interfaceNumber: number): Promise<void>;
  releaseInterface?(interfaceNumber: number): Promise<void>;
}

export interface SdrWebUsbLike {
  getDevices(): Promise<readonly SdrUsbDeviceLike[]>;
  requestDevice(options?: {
    readonly filters?: readonly SdrUsbFilter[];
  }): Promise<SdrUsbDeviceLike>;
}

export interface SdrStorageLike {
  getItem?(key: string): string | null;
  setItem?(key: string, value: string): void;
}

/** RTL2832U vendor/product pairs requested by the WebUSB RTL-SDR provider. */
export const RTL_SDR_USB_FILTERS: readonly SdrUsbFilter[] = Object.freeze([
  Object.freeze({ vendorId: 0x0bda, productId: 0x2832 }),
  Object.freeze({ vendorId: 0x0bda, productId: 0x2838 }),
]);

const ADSB_PRODUCT_PATTERN = /ads.?b|1090/i;
const NON_FM_PRODUCT_PATTERN = /ads.?b|1090|uat|978/i;

function text(value: unknown): string | null {
  return typeof value === 'string' && value.trim() ? value.trim() : null;
}

/**
 * Plain identity of a WebUSB device (or a stored identity).
 */
export function sdrDeviceIdentity(device: unknown): SdrDeviceIdentity | null {
  if (!device || typeof device !== 'object') return null;
  const d = device as Record<string, unknown>;
  const vendorId =
    typeof d.vendorId === 'number' && Number.isInteger(d.vendorId) ? d.vendorId : null;
  const productId =
    typeof d.productId === 'number' && Number.isInteger(d.productId) ? d.productId : null;
  return {
    vendorId,
    productId,
    productName: text(d.productName),
    serialNumber: text(d.serialNumber),
  };
}

/**
 * Whether two devices or identities name the same receiver channel.
 */
export function sameSdrDevice(a: unknown, b: unknown): boolean {
  const left = sdrDeviceIdentity(a);
  const right = sdrDeviceIdentity(b);
  return Boolean(
    left &&
      right &&
      left.vendorId === right.vendorId &&
      left.productId === right.productId &&
      left.productName === right.productName &&
      left.serialNumber === right.serialNumber,
  );
}

/**
 * Human-readable receiver label for status text.
 */
export function sdrDeviceLabel(device: unknown): string {
  const identity = sdrDeviceIdentity(device);
  if (!identity) return 'RTL-SDR';
  return identity.productName || 'RTL-SDR';
}

function filterMatchesDevice(filter: SdrUsbFilter, device: SdrUsbDeviceLike): boolean {
  return (
    (filter.vendorId === undefined || filter.vendorId === device.vendorId) &&
    (filter.productId === undefined || filter.productId === device.productId) &&
    (filter.serialNumber === undefined || filter.serialNumber === device.serialNumber)
  );
}

/**
 * Choose an already-authorized receiver without opening the WebUSB picker.
 * Order: the device remembered for this mode; then, for ADS-B, a channel
 * named for ADS-B/1090 MHz, or for FM a channel not named for ADS-B/UAT; then
 * the first matching device.
 */
export function selectAuthorizedSdrDevice<T extends SdrUsbDeviceLike = SdrUsbDeviceLike>(
  devices: readonly T[],
  {
    filters = [],
    mode = 'fm',
    remembered = null,
  }: {
    filters?: readonly SdrUsbFilter[];
    mode?: SdrReceiverMode;
    remembered?: unknown | null;
  } = {},
): T | null {
  const candidates = (Array.isArray(devices) ? devices : []).filter(
    (device): device is T =>
      Boolean(device) &&
      (filters.length === 0 || filters.some((filter) => filterMatchesDevice(filter, device))),
  );
  if (!candidates.length) return null;
  if (remembered) {
    const match = candidates.find((device) => sameSdrDevice(device, remembered));
    if (match) return match;
  }
  const preferred =
    mode === 'adsb'
      ? candidates.find((device) => ADSB_PRODUCT_PATTERN.test(device.productName || ''))
      : candidates.find((device) => !NON_FM_PRODUCT_PATTERN.test(device.productName || ''));
  return preferred || candidates[0] || null;
}

/**
 * Per-mode remembered receiver identities in browser storage.
 */
export function createSdrDeviceMemory(storage: SdrStorageLike | Storage | null): SdrDeviceMemory {
  const read = (): Record<string, unknown> => {
    try {
      const raw = storage?.getItem?.(SDR_DEVICE_STORAGE_KEY) || 'null';
      const value = JSON.parse(raw);
      return value && typeof value === 'object' ? (value as Record<string, unknown>) : {};
    } catch {
      return {};
    }
  };
  return {
    get(mode: SdrReceiverMode | string): SdrDeviceIdentity | null {
      return sdrDeviceIdentity(read()[mode]);
    },
    set(mode: SdrReceiverMode | string, device: unknown): void {
      const identity = sdrDeviceIdentity(device);
      if (!identity) return;
      try {
        storage?.setItem?.(SDR_DEVICE_STORAGE_KEY, JSON.stringify({ ...read(), [mode]: identity }));
      } catch {
        /* storage can be unavailable or quota-limited */
      }
    },
  };
}

export interface RememberingWebUsbOptions {
  getMode?: () => SdrReceiverMode;
  memory?: SdrDeviceMemory | null;
  consumeForcePicker?: () => boolean;
  onSelected?: (device: SdrUsbDeviceLike) => void;
}

/**
 * Wrap `navigator.usb` so the RTL-SDR provider reuses an authorized receiver
 * for the current mode before asking WebUSB to show its picker.
 */
export function createRememberingWebUsb<T extends SdrWebUsbLike = SdrWebUsbLike>(
  webUsb: T,
  {
    getMode = () => 'fm',
    memory = null,
    consumeForcePicker = () => false,
    onSelected = () => {},
  }: RememberingWebUsbOptions = {},
): T {
  if (!webUsb?.getDevices || !webUsb?.requestDevice) return webUsb;
  return new Proxy(webUsb, {
    get(target, property, receiver) {
      if (property === 'requestDevice') {
        return async (options: { readonly filters?: readonly SdrUsbFilter[] } = {}) => {
          const mode = getMode();
          const filters = Array.isArray(options.filters) ? options.filters : [];
          let device: SdrUsbDeviceLike | null = null;
          if (!consumeForcePicker()) {
            device = selectAuthorizedSdrDevice(await target.getDevices(), {
              filters,
              mode,
              remembered: memory?.get(mode) || null,
            });
          }
          if (!device) {
            // Only an explicit picker choice is remembered; automatic
            // selection is re-derived from the preference on every connect.
            device = await target.requestDevice(options);
            if (device) memory?.set(mode, device);
          }
          if (device) onSelected(device);
          return device;
        };
      }
      const value = Reflect.get(target, property, receiver);
      return typeof value === 'function'
        ? (value as (...args: unknown[]) => unknown).bind(target)
        : value;
    },
  });
}
