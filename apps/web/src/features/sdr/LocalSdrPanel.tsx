import { type ChangeEvent, useEffect, useId, useMemo, useState } from 'react';
import { Badge, Button, cx, SegmentedControl, TextField, WarningIcon } from '../../design';
import { R820T_GAIN_STEPS_DB, type SdrGainValue } from './gain';
import styles from './sdr.module.css';
import {
  getSharedSdrController,
  type SdrController,
  type SdrControllerState,
} from './sdrController';
import type { SdrReceiverMode } from './usbDevices';

export interface LocalSdrPanelProps {
  readonly controller?: SdrController | undefined;
  readonly className?: string | undefined;
}

export function LocalSdrPanel({ controller, className }: LocalSdrPanelProps) {
  const sdr = useMemo(() => controller ?? getSharedSdrController(), [controller]);
  const [state, setState] = useState<SdrControllerState>(() => sdr.getState());
  const [freqInput, setFreqInput] = useState<string>(() =>
    (sdr.getState().frequencyHz / 1_000_000).toFixed(3),
  );
  const [isFreqFocused, setIsFreqFocused] = useState(false);
  const gainSelectId = useId();

  useEffect(() => {
    return sdr.subscribe((next) => {
      setState(next);
      if (!isFreqFocused) {
        setFreqInput((next.frequencyHz / 1_000_000).toFixed(3));
      }
    });
  }, [sdr, isFreqFocused]);

  const handleConnect = async () => {
    await sdr.connect(state.mode);
  };

  const handleDisconnect = async () => {
    await sdr.disconnect();
  };

  const handleModeChange = async (newMode: string) => {
    const mode = newMode as SdrReceiverMode;
    await sdr.setMode(mode);
    setFreqInput((sdr.getState().frequencyHz / 1_000_000).toFixed(3));
  };

  const handleFrequencyBlur = () => {
    setIsFreqFocused(false);
    const mhz = Number.parseFloat(freqInput);
    if (Number.isFinite(mhz) && mhz > 0) {
      const hz = Math.round(mhz * 1_000_000);
      void sdr.setFrequency(hz);
    } else {
      setFreqInput((state.frequencyHz / 1_000_000).toFixed(3));
    }
  };

  const handleFrequencyKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.currentTarget.blur();
    }
  };

  const handleGainChange = (e: ChangeEvent<HTMLSelectElement>) => {
    const val = e.target.value;
    const gain: SdrGainValue = val === 'auto' ? 'auto' : Number(val);
    void sdr.setGain(gain);
  };

  const badgeTone = state.connected ? 'accent' : 'neutral';

  const badgeText = state.connected
    ? 'Connected'
    : state.status === 'connecting'
      ? 'Connecting…'
      : state.status === 'error'
        ? 'Error'
        : state.status === 'unsupported'
          ? 'Unsupported'
          : 'Disconnected';

  return (
    <article className={cx(styles.card, className)} aria-label="Local RTL-SDR Receiver">
      <header className={styles.header}>
        <div className={styles.titleGroup}>
          <h3 className={styles.title}>Local RTL-SDR</h3>
          <Badge tone={badgeTone}>{badgeText}</Badge>
        </div>
      </header>

      {/* WebUSB Browser Support Warning */}
      {!state.webUsbSupported && (
        <div className={styles.warningBox} role="alert">
          <WarningIcon size={16} weight="fill" className={styles.warningIcon} aria-hidden />
          <span>
            WebUSB is not supported in this browser. Use Chromium, Google Chrome, or Microsoft Edge
            to interface directly with local RTL2832U SDR dongles.
          </span>
        </div>
      )}

      {/* Error notification */}
      {state.status === 'error' && (
        <div className={styles.errorBox} role="alert">
          <span>{state.message}</span>
        </div>
      )}

      {/* Device details & connection status */}
      <div className={styles.deviceInfo}>
        {state.deviceLabel && <span className={styles.deviceLabel}>{state.deviceLabel}</span>}
        <p className={styles.statusMessage}>{state.message}</p>
      </div>

      {/* Receiver Controls */}
      <div className={styles.controls}>
        {/* Mode Selector */}
        <div className={styles.controlGroup}>
          <span className={styles.controlLabel}>Receiver Mode</span>
          <SegmentedControl
            aria-label="SDR receiver mode"
            options={[
              { value: 'adsb', label: 'ADS-B (1090 MHz)' },
              { value: 'fm', label: 'FM (87.5–108 MHz)' },
            ]}
            value={state.mode}
            onValueChange={handleModeChange}
          />
        </div>

        {/* Frequency Tune Input */}
        <div className={styles.controlGroup}>
          <TextField
            label="Center Frequency"
            size="sm"
            mono
            trailing="MHz"
            value={freqInput}
            onFocus={() => setIsFreqFocused(true)}
            onBlur={handleFrequencyBlur}
            onChange={(e) => setFreqInput(e.target.value)}
            onKeyDown={handleFrequencyKeyDown}
            aria-label="Tuned center frequency in megahertz"
          />
        </div>

        {/* Gain Selector */}
        <div className={styles.controlGroup}>
          <label htmlFor={gainSelectId} className={styles.controlLabel}>
            Tuner Gain
          </label>
          <select
            id={gainSelectId}
            className={styles.select}
            value={state.gain}
            onChange={handleGainChange}
            aria-label="Tuner gain setting"
          >
            <option value="auto">Auto (AGC)</option>
            {R820T_GAIN_STEPS_DB.map((step) => (
              <option key={step} value={step}>
                {step.toFixed(1)} dB
              </option>
            ))}
          </select>
        </div>
      </div>

      {/* Live Telemetry Stats */}
      <section className={styles.telemetryGrid} aria-label="Live SDR telemetry">
        <div className={styles.statCard}>
          <span className={styles.statLabel}>Message Rate</span>
          <span className={styles.statValue} data-testid="sdr-stat-rate">
            {state.messageRate.toFixed(1)} msg/s
          </span>
        </div>

        <div className={styles.statCard}>
          <span className={styles.statLabel}>Aircraft Heard</span>
          <span className={styles.statValue} data-testid="sdr-stat-aircraft">
            {state.aircraftCount}
          </span>
        </div>

        <div className={styles.statCard}>
          <span className={styles.statLabel}>Decoded Frames</span>
          <span className={styles.statValue} data-testid="sdr-stat-frames">
            {state.totalFrames}
          </span>
        </div>

        <div className={styles.statCard}>
          <span className={styles.statLabel}>I/Q Buffer Status</span>
          <span className={styles.statValue} data-testid="sdr-stat-buffer">
            {state.bufferStatus.toUpperCase()}
          </span>
        </div>
      </section>

      {/* Connect / Disconnect Buttons */}
      <div className={styles.actions}>
        {!state.connected ? (
          <Button
            variant="primary"
            disabled={!state.webUsbSupported || state.status === 'connecting'}
            onClick={handleConnect}
          >
            {state.status === 'connecting' ? 'Connecting…' : 'Connect RTL-SDR'}
          </Button>
        ) : (
          <>
            <Button variant="secondary" onClick={handleDisconnect}>
              Disconnect
            </Button>
            <Button variant="ghost" size="sm" onClick={() => void sdr.changeDevice()}>
              Change Device
            </Button>
          </>
        )}
      </div>
    </article>
  );
}
