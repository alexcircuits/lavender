export interface ReplaySample {
  readonly epochSec: number;
  readonly lat: number;
  readonly lon: number;
  readonly altitude?: number | undefined;
}

export type PlaybackSpeed = 1 | 2 | 4;

export interface ReplayInterpolatedPosition {
  readonly lat: number;
  readonly lon: number;
  readonly altitude?: number | undefined;
  readonly bearingDeg?: number | undefined;
  readonly currentSampleIndex: number;
}
