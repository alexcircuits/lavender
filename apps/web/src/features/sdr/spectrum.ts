/**
 * FFT power spectrum analysis and peak detection from interleaved unsigned 8-bit I/Q.
 */

export interface FmPeakResult {
  readonly frequency: number;
  readonly snr: number;
  readonly power: number;
}

export interface FindFmPeakOptions {
  readonly minFrequency?: number | undefined;
  readonly maxFrequency?: number | undefined;
  readonly thresholdDb?: number | undefined;
  readonly skipFrequency?: number | undefined;
}

function nextPowerOfTwo(value: number): number {
  return 2 ** Math.floor(Math.log2(Math.max(2, value)));
}

function fft(real: Float32Array, imag: Float32Array): void {
  const length = real.length;
  for (let index = 1, reversed = 0; index < length; index += 1) {
    let bit = length >> 1;
    while (reversed & bit) {
      reversed ^= bit;
      bit >>= 1;
    }
    reversed ^= bit;
    if (index >= reversed) continue;
    const tempReal = real[index] ?? 0;
    const tempImag = imag[index] ?? 0;
    real[index] = real[reversed] ?? 0;
    imag[index] = imag[reversed] ?? 0;
    real[reversed] = tempReal;
    imag[reversed] = tempImag;
  }
  for (let size = 2; size <= length; size <<= 1) {
    const angle = (-2 * Math.PI) / size;
    const stepReal = Math.cos(angle);
    const stepImag = Math.sin(angle);
    for (let start = 0; start < length; start += size) {
      let twiddleReal = 1;
      let twiddleImag = 0;
      for (let index = 0; index < size / 2; index += 1) {
        const even = start + index;
        const odd = even + size / 2;
        const rOdd = real[odd] ?? 0;
        const iOdd = imag[odd] ?? 0;
        const rEven = real[even] ?? 0;
        const iEven = imag[even] ?? 0;
        const oddReal = rOdd * twiddleReal - iOdd * twiddleImag;
        const oddImag = rOdd * twiddleImag + iOdd * twiddleReal;
        real[odd] = rEven - oddReal;
        imag[odd] = iEven - oddImag;
        real[even] = rEven + oddReal;
        imag[even] = iEven + oddImag;
        const nextReal = twiddleReal * stepReal - twiddleImag * stepImag;
        twiddleImag = twiddleReal * stepImag + twiddleImag * stepReal;
        twiddleReal = nextReal;
      }
    }
  }
}

/**
 * Create an FFT-shifted dB power spectrum from interleaved unsigned 8-bit IQ.
 */
export function analyzeSpectrum(
  buffer: ArrayBuffer | ArrayBufferView,
  maxBins = 2_048,
): Float32Array {
  const iq =
    buffer instanceof Uint8Array
      ? buffer
      : ArrayBuffer.isView(buffer)
        ? new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength)
        : new Uint8Array(buffer);
  if (iq.length < 4) return new Float32Array(0);
  const length = Math.min(nextPowerOfTwo(Math.floor(iq.length / 2)), maxBins);
  if (length < 2) return new Float32Array(0);

  const real = new Float32Array(length);
  const imag = new Float32Array(length);
  const sampleOffset = Math.max(0, Math.floor((iq.length / 2 - length) / 2));

  for (let index = 0; index < length; index += 1) {
    const window = 0.5 - 0.5 * Math.cos((2 * Math.PI * index) / (length - 1));
    const iSample = iq[(sampleOffset + index) * 2] ?? 127.5;
    const qSample = iq[(sampleOffset + index) * 2 + 1] ?? 127.5;
    real[index] = ((iSample - 127.5) / 128) * window;
    imag[index] = ((qSample - 127.5) / 128) * window;
  }

  fft(real, imag);

  const bins = new Float32Array(length);
  for (let index = 0; index < length; index += 1) {
    const shifted = (index + length / 2) % length;
    const r = real[shifted] ?? 0;
    const i = imag[shifted] ?? 0;
    const power = (r * r + i * i) / (length * length);
    bins[index] = 10 * Math.log10(power + 1e-12);
  }
  return bins;
}

function median(values: Float32Array | number[]): number {
  const sorted = Array.from(values).sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)] ?? -120;
}

function channelPower(bins: Float32Array, center: number, radius: number): number {
  let total = 0;
  let count = 0;
  const start = Math.max(0, center - radius);
  const end = Math.min(bins.length - 1, center + radius);
  for (let index = start; index <= end; index += 1) {
    total += 10 ** ((bins[index] ?? -120) / 10);
    count += 1;
  }
  return 10 * Math.log10(total / Math.max(count, 1) + 1e-12);
}

/**
 * Find the nearest directional broadcast-FM channel peak in one spectrum frame.
 */
export function findDirectionalFmPeak(
  bins: Float32Array,
  centerFrequency: number,
  sampleRate: number,
  direction: number,
  afterFrequency: number,
  {
    minFrequency = 87_500_000,
    maxFrequency = 108_000_000,
    thresholdDb = 6,
    skipFrequency = Number.NaN,
  }: FindFmPeakOptions = {},
): FmPeakResult | null {
  if (!(bins instanceof Float32Array) || bins.length < 16 || !Number.isFinite(sampleRate)) {
    return null;
  }
  const binWidth = sampleRate / bins.length;
  const channelRadius = Math.max(1, Math.round(55_000 / binWidth));
  const comparisonDistance = Math.max(channelRadius + 1, Math.round(150_000 / binWidth));
  const powers = new Float32Array(bins.length);
  for (let index = 0; index < bins.length; index += 1) {
    powers[index] = channelPower(bins, index, channelRadius);
  }
  const noiseFloor = median(powers);
  const candidates: FmPeakResult[] = [];
  const frameMin = centerFrequency - sampleRate / 2;
  const frameMax = centerFrequency + sampleRate / 2;
  const firstChannel = Math.ceil(Math.max(minFrequency, frameMin) / 100_000) * 100_000;
  const lastChannel = Math.floor(Math.min(maxFrequency, frameMax) / 100_000) * 100_000;

  for (let frequency = firstChannel; frequency <= lastChannel; frequency += 100_000) {
    if (direction > 0 && frequency <= afterFrequency + 75_000) continue;
    if (direction < 0 && frequency >= afterFrequency - 75_000) continue;
    if (Math.abs(frequency - skipFrequency) <= 75_000) continue;
    const index = Math.round((frequency - centerFrequency) / binWidth + bins.length / 2);
    if (index < comparisonDistance || index >= bins.length - comparisonDistance) {
      continue;
    }
    const power = powers[index] ?? -120;
    if (power - noiseFloor < thresholdDb) continue;
    const lowerNeighbor = powers[index - comparisonDistance] ?? -120;
    const upperNeighbor = powers[index + comparisonDistance] ?? -120;
    if (power < lowerNeighbor || power < upperNeighbor) {
      continue;
    }
    candidates.push({ frequency, snr: power - noiseFloor, power });
  }

  candidates.sort(
    (a, b) =>
      Math.abs(a.frequency - afterFrequency) - Math.abs(b.frequency - afterFrequency) ||
      b.snr - a.snr,
  );
  return candidates[0] ?? null;
}
