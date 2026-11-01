let egm96Module: typeof import('egm96-universal') | null = null;
let readyPromise: Promise<void> | null = null;

/**
 * Lazily loads the EGM96 grid (dynamic import — code-split so the ~2.7 MB grid
 * data stays out of the eager main bundle). Safe to call repeatedly; the underlying
 * import happens once and subsequent calls resolve from the cached promise.
 */
export async function ensureGeoidReady(): Promise<void> {
  if (!readyPromise) {
    readyPromise = import('egm96-universal').then((mod) => {
      egm96Module = mod;
    });
  }
  return readyPromise;
}

/**
 * Returns whether the EGM96 geoid module has been loaded and is ready for lookups.
 */
export function isGeoidReady(): boolean {
  return egm96Module !== null;
}

/**
 * Geoid undulation N at a given point, in metres, relative to the WGS84
 * ellipsoid (positive = geoid above ellipsoid). Throws if ensureGeoidReady()
 * has not resolved yet.
 */
export function geoidHeight(latDeg: number, lonDeg: number): number {
  if (!egm96Module) {
    throw new Error(
      'geoid.ts: geoidHeight() called before ensureGeoidReady() resolved — await ensureGeoidReady() first.',
    );
  }
  return egm96Module.meanSeaLevel(latDeg, lonDeg);
}

/**
 * Converts an orthometric (mean-sea-level) height to an ellipsoidal
 * (WGS84 globe-relative) height: h = H + N.
 */
export function orthometricToEllipsoidal(hMslM: number, latDeg: number, lonDeg: number): number {
  return hMslM + geoidHeight(latDeg, lonDeg);
}

/**
 * READOUT-ONLY inverse of orthometricToEllipsoidal: H = h - N.
 *
 * Cesium reports camera and entity heights against the WGS84 ellipsoid (h),
 * but a viewer reads "ALT" as height above Mean Sea Level (H).
 * Returns the uncorrected height if geoidUndulationM is not finite.
 */
export function ellipsoidalToMslDisplayM(
  hEllipsoidalM: number,
  geoidUndulationM: number | null | undefined,
): number;
export function ellipsoidalToMslDisplayM(
  hEllipsoidalM: number | null | undefined,
  geoidUndulationM: number | null | undefined,
): number | null | undefined;
export function ellipsoidalToMslDisplayM(
  hEllipsoidalM: number | null | undefined,
  geoidUndulationM: number | null | undefined,
): number | null | undefined {
  if (typeof hEllipsoidalM !== 'number' || !Number.isFinite(hEllipsoidalM)) return hEllipsoidalM;
  if (typeof geoidUndulationM !== 'number' || !Number.isFinite(geoidUndulationM))
    return hEllipsoidalM;
  return hEllipsoidalM - geoidUndulationM;
}
