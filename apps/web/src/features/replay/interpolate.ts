import type { ReplayInterpolatedPosition, ReplaySample } from './types';

/** Calculates initial forward azimuth / bearing in degrees [0, 360). */
export function calculateBearing(lat1: number, lon1: number, lat2: number, lon2: number): number {
  const toRad = Math.PI / 180;
  const phi1 = lat1 * toRad;
  const phi2 = lat2 * toRad;
  const deltaLon = (lon2 - lon1) * toRad;
  const y = Math.sin(deltaLon) * Math.cos(phi2);
  const x = Math.cos(phi1) * Math.sin(phi2) - Math.sin(phi1) * Math.cos(phi2) * Math.cos(deltaLon);
  const theta = Math.atan2(y, x);
  return ((theta * 180) / Math.PI + 360) % 360;
}

/** Interpolates kinematics along a chronological series of samples. */
export function interpolateSample(
  samples: readonly ReplaySample[],
  epochSec: number,
): ReplayInterpolatedPosition | null {
  if (samples.length === 0) return null;
  const first = samples[0];
  if (!first) return null;

  if (samples.length === 1 || epochSec <= first.epochSec) {
    return {
      lat: first.lat,
      lon: first.lon,
      altitude: first.altitude,
      currentSampleIndex: 0,
    };
  }

  const lastIndex = samples.length - 1;
  const last = samples[lastIndex];
  if (!last) return null;

  if (epochSec >= last.epochSec) {
    const prev = samples[lastIndex - 1];
    return {
      lat: last.lat,
      lon: last.lon,
      altitude: last.altitude,
      bearingDeg: prev ? calculateBearing(prev.lat, prev.lon, last.lat, last.lon) : undefined,
      currentSampleIndex: lastIndex,
    };
  }

  // Find interval: samples[i].epochSec <= epochSec <= samples[i+1].epochSec
  let i = 0;
  while (i < lastIndex - 1) {
    const nextSample = samples[i + 1];
    if (nextSample && nextSample.epochSec < epochSec) {
      i++;
    } else {
      break;
    }
  }

  const s0 = samples[i];
  const s1 = samples[i + 1];
  if (!s0 || !s1) return null;

  const duration = s1.epochSec - s0.epochSec;
  const alpha = duration > 0 ? (epochSec - s0.epochSec) / duration : 0;

  // Linear lat interpolation
  const lat = s0.lat + alpha * (s1.lat - s0.lat);

  // Antimeridian-aware lon interpolation
  let deltaLon = s1.lon - s0.lon;
  if (deltaLon > 180) deltaLon -= 360;
  else if (deltaLon < -180) deltaLon += 360;
  let lon = s0.lon + alpha * deltaLon;
  if (lon > 180) lon -= 360;
  else if (lon < -180) lon += 360;

  // Altitude interpolation
  let altitude: number | undefined;
  if (typeof s0.altitude === 'number' && typeof s1.altitude === 'number') {
    altitude = s0.altitude + alpha * (s1.altitude - s0.altitude);
  } else {
    altitude = s0.altitude ?? s1.altitude;
  }

  const bearingDeg = calculateBearing(s0.lat, s0.lon, s1.lat, s1.lon);

  return {
    lat,
    lon,
    altitude,
    bearingDeg,
    currentSampleIndex: i,
  };
}
