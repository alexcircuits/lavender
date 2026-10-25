import { useEffect, useMemo, useRef, useState } from 'react';
import type { AircraftTrack, ApiError, VesselTrackResponse } from '../../contracts';
import {
  Button,
  formatAge,
  IconButton,
  Panel,
  PauseIcon,
  PlayIcon,
  Table,
  type TableColumn,
  XIcon,
} from '../../design';
import { formatDecimal } from '../../lib/coordinates';
import { formatUtcTime } from '../../lib/time';
import { toApiError } from '../../platform';
import { useGlobe, usePlatform, useUi, useWorkspace } from '../../runtime';
import { interpolateSample } from './interpolate';
import styles from './replay.module.css';
import type { PlaybackSpeed, ReplaySample } from './types';

const SPEED_OPTIONS: readonly PlaybackSpeed[] = [1, 2, 4];
const CAMERA_FOLLOW_INTERVAL_MS = 500;

const TABLE_COLUMNS: readonly TableColumn<ReplaySample>[] = [
  {
    id: 'time',
    header: 'Observed (UTC)',
    mono: true,
    cell: (s) => formatUtcTime(s.epochSec * 1000),
  },
  {
    id: 'coords',
    header: 'Position',
    mono: true,
    cell: (s) => formatDecimal({ lat: s.lat, lon: s.lon }),
  },
  {
    id: 'alt',
    header: 'Altitude',
    mono: true,
    cell: (s) => (typeof s.altitude === 'number' ? `${Math.round(s.altitude)} m` : 'Surface'),
  },
];

export function ReplayPanel({ className }: { readonly className?: string | undefined }) {
  const platform = usePlatform();
  const globe = useGlobe();
  const setSidePanel = useUi((s) => s.setSidePanel);
  const selection = useWorkspace((s) => s.selection);

  const [pending, setPending] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [samples, setSamples] = useState<readonly ReplaySample[]>([]);
  const [aircraftTrack, setAircraftTrack] = useState<AircraftTrack | null>(null);
  const [vesselTrack, setVesselTrack] = useState<VesselTrackResponse | null>(null);

  const [currentTime, setCurrentTime] = useState(0);
  const [isPlaying, setIsPlaying] = useState(false);
  const [speed, setSpeed] = useState<PlaybackSpeed>(1);
  const [cameraFollow, setCameraFollow] = useState(false);

  const lastCameraFlyTimeRef = useRef(0);

  const isReplayable = selection && (selection.feed === 'aircraft' || selection.feed === 'vessels');

  // Load track when selection changes
  useEffect(() => {
    if (!selection || (selection.feed !== 'aircraft' && selection.feed !== 'vessels')) {
      setSamples([]);
      setAircraftTrack(null);
      setVesselTrack(null);
      setError(null);
      setPending(false);
      setIsPlaying(false);
      return;
    }

    const controller = new AbortController();
    setPending(true);
    setError(null);
    setIsPlaying(false);

    if (selection.feed === 'aircraft') {
      platform.aircraftTrack(selection.id, { signal: controller.signal }).then(
        (track) => {
          if (controller.signal.aborted) return;
          setPending(false);
          setAircraftTrack(track);
          setVesselTrack(null);

          const parsed: ReplaySample[] = track.points
            .map((p) => ({
              epochSec: Math.floor(new Date(p.time).getTime() / 1000),
              lat: p.position.lat,
              lon: p.position.lon,
              altitude: p.position.altM,
            }))
            .filter(
              (s) =>
                Number.isFinite(s.epochSec) && Number.isFinite(s.lat) && Number.isFinite(s.lon),
            )
            .sort((a, b) => a.epochSec - b.epochSec);

          setSamples(parsed);
          const first = parsed[0];
          if (first) {
            setCurrentTime(first.epochSec);
          }
        },
        (cause) => {
          if (controller.signal.aborted) return;
          setPending(false);
          setError(toApiError(cause));
        },
      );
    } else {
      platform.vesselTrack(selection.id, { signal: controller.signal }).then(
        (track) => {
          if (controller.signal.aborted) return;
          setPending(false);
          setVesselTrack(track);
          setAircraftTrack(null);

          const parsed: ReplaySample[] = track.samples
            .map((s) => ({
              epochSec: s.epochSec,
              lat: s.lat,
              lon: s.lon,
            }))
            .filter(
              (s) =>
                Number.isFinite(s.epochSec) && Number.isFinite(s.lat) && Number.isFinite(s.lon),
            )
            .sort((a, b) => a.epochSec - b.epochSec);

          setSamples(parsed);
          const first = parsed[0];
          if (first) {
            setCurrentTime(first.epochSec);
          }
        },
        (cause) => {
          if (controller.signal.aborted) return;
          setPending(false);
          setError(toApiError(cause));
        },
      );
    }

    return () => controller.abort();
  }, [platform, selection]);

  // Mirror VesselTrack globe rendering
  useEffect(() => {
    if (!globe) return;
    if (aircraftTrack) {
      globe.showAircraftTrack(aircraftTrack);
      return () => globe.showAircraftTrack(null);
    }
    if (vesselTrack) {
      globe.showVesselTrack(vesselTrack);
      return () => globe.showVesselTrack(null);
    }
  }, [globe, aircraftTrack, vesselTrack]);

  const startTime = samples[0]?.epochSec ?? 0;
  const endTime = samples[samples.length - 1]?.epochSec ?? 0;
  const durationSec = endTime - startTime;

  // Playback timer
  useEffect(() => {
    if (!isPlaying || samples.length < 2) return;

    const interval = setInterval(() => {
      setCurrentTime((prev) => {
        // Step forward by (100ms * speed) in track time seconds
        const stepSec = 0.1 * speed;
        const next = prev + stepSec;
        if (next >= endTime) {
          setIsPlaying(false);
          return endTime;
        }
        return next;
      });
    }, 100);

    return () => clearInterval(interval);
  }, [isPlaying, speed, samples.length, endTime]);

  // Interpolated position at currentTime
  const interpolated = useMemo(
    () => interpolateSample(samples, currentTime),
    [samples, currentTime],
  );

  // Camera follow loop
  useEffect(() => {
    if (!cameraFollow || !globe || !interpolated) return;

    const now = performance.now();
    if (now - lastCameraFlyTimeRef.current >= CAMERA_FOLLOW_INTERVAL_MS) {
      lastCameraFlyTimeRef.current = now;
      globe.flyTo({
        kind: 'point',
        position: {
          lat: interpolated.lat,
          lon: interpolated.lon,
          ...(typeof interpolated.altitude === 'number' ? { altM: interpolated.altitude } : {}),
        },
        heightM: interpolated.altitude ? interpolated.altitude + 2500 : 4000,
      });
    }
  }, [cameraFollow, globe, interpolated]);

  const handleSeek = (value: number) => {
    setCurrentTime(value);
  };

  const handleReset = () => {
    setCurrentTime(startTime);
    setIsPlaying(false);
  };

  const handleJumpToEnd = () => {
    setCurrentTime(endTime);
    setIsPlaying(false);
  };

  return (
    <Panel
      title="Timeline Replay"
      dense
      className={className}
      actions={
        <IconButton label="Close timeline replay" icon={XIcon} onClick={() => setSidePanel(null)} />
      }
    >
      <div className={styles.container}>
        {!isReplayable ? (
          <div className={styles.emptyPrompt}>
            <strong>No replayable entity selected.</strong>
            <span>
              Select an aircraft or vessel on the 3D globe or search by callsign/MMSI to inspect and
              replay its retained historical trajectory.
            </span>
          </div>
        ) : (
          <>
            <div className={styles.entityHeader}>
              <div className={styles.entityMeta}>
                <span className={styles.entityKind}>{selection.feed}</span>
                <span className={styles.entityId}>{selection.id}</span>
              </div>
              {pending ? <span>Loading track…</span> : null}
            </div>

            {error ? (
              <div role="alert" className={styles.emptyPrompt}>
                {error.code === 'notFound'
                  ? 'No historical positions retained for this entity yet.'
                  : error.message}
              </div>
            ) : null}

            {samples.length >= 2 ? (
              <>
                <div className={styles.telemetryCard}>
                  <div className={styles.telemetryItem}>
                    <span className={styles.telemetryLabel}>Latitude</span>
                    <span className={styles.telemetryValue}>
                      {interpolated ? interpolated.lat.toFixed(5) : '—'}
                    </span>
                  </div>
                  <div className={styles.telemetryItem}>
                    <span className={styles.telemetryLabel}>Longitude</span>
                    <span className={styles.telemetryValue}>
                      {interpolated ? interpolated.lon.toFixed(5) : '—'}
                    </span>
                  </div>
                  <div className={styles.telemetryItem}>
                    <span className={styles.telemetryLabel}>Altitude</span>
                    <span className={styles.telemetryValue}>
                      {typeof interpolated?.altitude === 'number'
                        ? `${Math.round(interpolated.altitude)} m`
                        : 'Surface'}
                    </span>
                  </div>
                  <div className={styles.telemetryItem}>
                    <span className={styles.telemetryLabel}>Heading</span>
                    <span className={styles.telemetryValue}>
                      {typeof interpolated?.bearingDeg === 'number'
                        ? `${Math.round(interpolated.bearingDeg)}°`
                        : '—'}
                    </span>
                  </div>
                </div>

                <div className={styles.scrubberSection}>
                  <div className={styles.timeDisplay}>
                    <span>{formatUtcTime(currentTime * 1000)}</span>
                    <span>Span: {formatAge(durationSec)}</span>
                  </div>

                  <input
                    type="range"
                    aria-label="Replay timeline scrubber"
                    className={styles.scrubberTrack}
                    min={startTime}
                    max={endTime}
                    step={1}
                    value={currentTime}
                    onChange={(e) => handleSeek(Number(e.target.value))}
                  />
                </div>

                <div className={styles.controlsRow}>
                  <div className={styles.playbackButtons}>
                    <IconButton
                      label={isPlaying ? 'Pause replay' : 'Play replay'}
                      icon={isPlaying ? PauseIcon : PlayIcon}
                      onClick={() => setIsPlaying((p) => !p)}
                    />
                    <Button variant="ghost" size="sm" onClick={handleReset}>
                      Start
                    </Button>
                    <Button variant="ghost" size="sm" onClick={handleJumpToEnd}>
                      End
                    </Button>
                  </div>

                  <fieldset className={styles.speedGroup} aria-label="Playback speed">
                    {SPEED_OPTIONS.map((s) => (
                      <button
                        key={s}
                        type="button"
                        className={`${styles.speedButton} ${speed === s ? styles.speedButtonActive : ''}`}
                        onClick={() => setSpeed(s)}
                        aria-pressed={speed === s}
                      >
                        {s}×
                      </button>
                    ))}
                  </fieldset>
                </div>

                <label className={styles.followToggle}>
                  <input
                    type="checkbox"
                    className={styles.followCheckbox}
                    checked={cameraFollow}
                    onChange={(e) => setCameraFollow(e.target.checked)}
                  />
                  <span>Follow entity with camera</span>
                </label>

                <div className={styles.historySummary}>
                  {samples.length} positions retained over {formatAge(durationSec)}.
                </div>

                <Table
                  aria-label="Historical track samples"
                  columns={TABLE_COLUMNS}
                  rows={[...samples].reverse()}
                  getRowId={(s) => `${s.epochSec}:${s.lat},${s.lon}`}
                  maxHeight="200px"
                  minWidth="280px"
                />
              </>
            ) : !pending && !error ? (
              <div className={styles.emptyPrompt}>
                Insufficient historical points retained to replay (need at least 2 observations).
              </div>
            ) : null}
          </>
        )}
      </div>
    </Panel>
  );
}
