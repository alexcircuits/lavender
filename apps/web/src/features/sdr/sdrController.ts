/**
 * Lightweight RTL-SDR manager for WebUSB receiver control and ADS-B telemetry decoding.
 */

import {
  AdsbStreamDecoder,
  type DecodedAdsbMessage,
  decodeAdsbMessage,
  LOCAL_ADSB_STALE_MS,
} from '../../globe/renderers/wasmSdr';
import {
  normalizeSdrGain,
  readSdrGainSettings,
  SDR_GAIN_DEFAULTS,
  type SdrGainSettings,
  type SdrGainValue,
  writeSdrGainSettings,
} from './gain';
import {
  createRememberingWebUsb,
  createSdrDeviceMemory,
  RTL_SDR_USB_FILTERS,
  type SdrDeviceIdentity,
  type SdrDeviceMemory,
  type SdrReceiverMode,
  type SdrStorageLike,
  type SdrUsbDeviceLike,
  type SdrWebUsbLike,
  sdrDeviceIdentity,
  sdrDeviceLabel,
} from './usbDevices';

export type SdrConnectionStatus = 'idle' | 'connecting' | 'connected' | 'error' | 'unsupported';
export type SdrBufferStatus = 'idle' | 'ok' | 'overflow' | 'underflow';

export interface SdrPreset {
  readonly frequencyHz: number;
  readonly sampleRate: number;
}

export const SDR_PRESETS: Readonly<Record<SdrReceiverMode, SdrPreset>> = Object.freeze({
  adsb: Object.freeze({
    frequencyHz: 1_090_000_000,
    sampleRate: 2_000_000,
  }),
  fm: Object.freeze({
    frequencyHz: 98_500_000,
    sampleRate: 2_048_000,
  }),
});

export interface SdrControllerState {
  readonly webUsbSupported: boolean;
  readonly connected: boolean;
  readonly status: SdrConnectionStatus;
  readonly message: string;
  readonly mode: SdrReceiverMode;
  readonly frequencyHz: number;
  readonly sampleRate: number;
  readonly gain: SdrGainValue;
  readonly deviceLabel: string | null;
  readonly deviceIdentity: SdrDeviceIdentity | null;
  // Live telemetry stats
  readonly messageRate: number;
  readonly totalFrames: number;
  readonly aircraftCount: number;
  readonly bufferStatus: SdrBufferStatus;
  readonly iqBytesReceived: number;
  readonly lastMessageAt: number | null;
}

export interface TrackedAircraft {
  readonly icao: string;
  readonly lastSeen: number;
  readonly message: DecodedAdsbMessage;
}

export interface SdrControllerOptions {
  readonly webUsb?: SdrWebUsbLike | null | undefined;
  readonly storage?: SdrStorageLike | Storage | null | undefined;
  readonly decoder?: AdsbStreamDecoder | undefined;
}

function resolveDefaultWebUsb(): SdrWebUsbLike | null {
  if (typeof navigator !== 'undefined' && 'usb' in navigator) {
    return (navigator as unknown as { usb: SdrWebUsbLike }).usb;
  }
  return null;
}

function resolveDefaultStorage(): SdrStorageLike | Storage | null {
  if (typeof window !== 'undefined' && window.localStorage) {
    return window.localStorage;
  }
  return null;
}

export class SdrController {
  #webUsb: SdrWebUsbLike | null;
  #storage: SdrStorageLike | Storage | null;
  #deviceMemory: SdrDeviceMemory;
  #gainSettings: SdrGainSettings;
  #decoder: AdsbStreamDecoder;

  #activeDevice: SdrUsbDeviceLike | null = null;
  #deviceIdentity: SdrDeviceIdentity | null = null;
  #listeners = new Set<(state: SdrControllerState) => void>();

  #state: SdrControllerState;
  #trackedAircraft = new Map<string, TrackedAircraft>();
  #messageTimestamps: number[] = [];
  #totalFrames = 0;
  #iqBytesReceived = 0;
  #lastMessageAt: number | null = null;
  #forcePicker = false;

  constructor(options: SdrControllerOptions = {}) {
    this.#webUsb = options.webUsb ?? resolveDefaultWebUsb();
    this.#storage = options.storage ?? resolveDefaultStorage();
    this.#decoder = options.decoder ?? new AdsbStreamDecoder();

    this.#deviceMemory = createSdrDeviceMemory(this.#storage);
    this.#gainSettings = readSdrGainSettings(this.#storage);

    const webUsbSupported = Boolean(this.#webUsb);
    const initialMode: SdrReceiverMode = 'adsb';
    const preset = SDR_PRESETS[initialMode];

    this.#state = {
      webUsbSupported,
      connected: false,
      status: webUsbSupported ? 'idle' : 'unsupported',
      message: webUsbSupported
        ? 'Connect an RTL-SDR receiver to begin.'
        : 'WebUSB is not supported in this browser.',
      mode: initialMode,
      frequencyHz: preset.frequencyHz,
      sampleRate: preset.sampleRate,
      gain: this.#gainSettings[initialMode] ?? SDR_GAIN_DEFAULTS[initialMode],
      deviceLabel: null,
      deviceIdentity: null,
      messageRate: 0,
      totalFrames: 0,
      aircraftCount: 0,
      bufferStatus: 'idle',
      iqBytesReceived: 0,
      lastMessageAt: null,
    };
  }

  public getState(): SdrControllerState {
    return this.#state;
  }

  public subscribe(listener: (state: SdrControllerState) => void): () => void {
    this.#listeners.add(listener);
    listener(this.#state);
    return () => {
      this.#listeners.delete(listener);
    };
  }

  #setState(patch: Partial<SdrControllerState>): void {
    this.#state = { ...this.#state, ...patch };
    for (const listener of this.#listeners) {
      try {
        listener(this.#state);
      } catch {
        /* listener errors are suppressed */
      }
    }
  }

  public async connect(mode: SdrReceiverMode = this.#state.mode): Promise<boolean> {
    if (!this.#state.webUsbSupported || !this.#webUsb) {
      this.#setState({
        status: 'unsupported',
        message: 'WebUSB is not supported in this browser.',
      });
      return false;
    }

    if (this.#activeDevice) {
      if (this.#state.mode !== mode) {
        await this.setMode(mode);
      }
      return true;
    }

    this.#setState({
      status: 'connecting',
      message: 'Requesting RTL-SDR device...',
    });

    try {
      const remembering = createRememberingWebUsb(this.#webUsb, {
        getMode: () => mode,
        memory: this.#deviceMemory,
        consumeForcePicker: () => {
          const force = this.#forcePicker;
          this.#forcePicker = false;
          return force;
        },
        onSelected: (dev) => {
          this.#deviceIdentity = sdrDeviceIdentity(dev);
        },
      });

      const device = await remembering.requestDevice({ filters: RTL_SDR_USB_FILTERS });
      if (!device) {
        this.#setState({
          status: 'idle',
          message: 'No device selected.',
        });
        return false;
      }

      if (typeof device.open === 'function') {
        await device.open();
      }
      if (typeof device.selectConfiguration === 'function') {
        await device.selectConfiguration(1);
      }
      if (typeof device.claimInterface === 'function') {
        await device.claimInterface(0);
      }

      this.#activeDevice = device;
      const identity = this.#deviceIdentity ?? sdrDeviceIdentity(device);
      const label = sdrDeviceLabel(device);
      const preset = SDR_PRESETS[mode];
      const gain = this.#gainSettings[mode] ?? SDR_GAIN_DEFAULTS[mode];

      this.#setState({
        connected: true,
        status: 'connected',
        message: `Connected to ${label}.`,
        deviceLabel: label,
        deviceIdentity: identity,
        mode,
        frequencyHz: preset.frequencyHz,
        sampleRate: preset.sampleRate,
        gain,
        bufferStatus: 'idle',
      });

      return true;
    } catch (error) {
      const err = error as Error | undefined;
      const msg = err?.message ?? String(error);
      const isAbort = /notfounderror|user cancelled|no device selected|abort/i.test(msg);

      this.#setState({
        connected: false,
        status: isAbort ? 'idle' : 'error',
        message: isAbort ? 'Device selection cancelled.' : `Connection failed: ${msg}`,
      });
      return false;
    }
  }

  public async disconnect(): Promise<void> {
    if (this.#activeDevice && typeof this.#activeDevice.close === 'function') {
      try {
        await this.#activeDevice.close();
      } catch {
        /* already closed */
      }
    }
    this.#activeDevice = null;
    this.#deviceIdentity = null;
    this.#decoder.reset();

    this.#setState({
      connected: false,
      status: this.#state.webUsbSupported ? 'idle' : 'unsupported',
      message: 'Disconnected from RTL-SDR.',
      deviceLabel: null,
      deviceIdentity: null,
      bufferStatus: 'idle',
      messageRate: 0,
    });
  }

  public async changeDevice(): Promise<boolean> {
    this.#forcePicker = true;
    await this.disconnect();
    return this.connect(this.#state.mode);
  }

  public async setMode(mode: SdrReceiverMode): Promise<void> {
    const preset = SDR_PRESETS[mode];
    const gain = this.#gainSettings[mode] ?? SDR_GAIN_DEFAULTS[mode];
    this.#decoder.reset();
    this.#trackedAircraft.clear();
    this.#messageTimestamps = [];

    this.#setState({
      mode,
      frequencyHz: preset.frequencyHz,
      sampleRate: preset.sampleRate,
      gain,
      messageRate: 0,
      aircraftCount: 0,
    });
  }

  public async setFrequency(frequencyHz: number): Promise<void> {
    if (!Number.isFinite(frequencyHz) || frequencyHz <= 0) return;
    this.#setState({ frequencyHz });
  }

  public async setGain(gainValue: SdrGainValue): Promise<void> {
    const normalized = normalizeSdrGain(gainValue);
    if (normalized === null) return;

    this.#gainSettings = {
      ...this.#gainSettings,
      [this.#state.mode]: normalized,
    };
    writeSdrGainSettings(this.#storage, this.#gainSettings);
    this.#setState({ gain: normalized });
  }

  public feedIqData(
    buffer: ArrayBuffer | Uint8Array,
    sampleRate: number = this.#state.sampleRate,
  ): Uint8Array[] {
    const iq = buffer instanceof Uint8Array ? buffer : new Uint8Array(buffer);
    const now = Date.now();
    this.#iqBytesReceived += iq.byteLength;

    const frames = this.#decoder.extract(iq, sampleRate);

    if (frames.length > 0) {
      this.#totalFrames += frames.length;
      this.#lastMessageAt = now;
      for (let i = 0; i < frames.length; i++) {
        this.#messageTimestamps.push(now);
      }

      for (const frame of frames) {
        const decoded = decodeAdsbMessage(frame, { receivedAt: now });
        if (decoded?.icao) {
          this.#trackedAircraft.set(decoded.icao, {
            icao: decoded.icao,
            lastSeen: now,
            message: decoded,
          });
        }
      }
    }

    // Rolling 1-second message rate window
    const windowStart = now - 1000;
    while (this.#messageTimestamps.length > 0 && (this.#messageTimestamps[0] ?? 0) < windowStart) {
      this.#messageTimestamps.shift();
    }
    const messageRate = this.#messageTimestamps.length;

    // Prune stale aircraft older than 60s
    const staleThreshold = now - LOCAL_ADSB_STALE_MS;
    for (const [icao, info] of this.#trackedAircraft.entries()) {
      if (info.lastSeen < staleThreshold) {
        this.#trackedAircraft.delete(icao);
      }
    }

    const bufferStatus: SdrBufferStatus = frames.length > 0 || iq.byteLength > 0 ? 'ok' : 'idle';

    this.#setState({
      totalFrames: this.#totalFrames,
      aircraftCount: this.#trackedAircraft.size,
      messageRate,
      bufferStatus,
      iqBytesReceived: this.#iqBytesReceived,
      lastMessageAt: this.#lastMessageAt,
    });

    return frames;
  }

  public getTrackedAircraft(): readonly TrackedAircraft[] {
    return Array.from(this.#trackedAircraft.values());
  }

  public resetStats(): void {
    this.#trackedAircraft.clear();
    this.#messageTimestamps = [];
    this.#totalFrames = 0;
    this.#iqBytesReceived = 0;
    this.#lastMessageAt = null;
    this.#decoder.reset();

    this.#setState({
      totalFrames: 0,
      aircraftCount: 0,
      messageRate: 0,
      iqBytesReceived: 0,
      lastMessageAt: null,
      bufferStatus: 'idle',
    });
  }
}

let sharedSdrController: SdrController | null = null;

export function getSharedSdrController(): SdrController {
  if (!sharedSdrController) {
    sharedSdrController = new SdrController();
  }
  return sharedSdrController;
}
