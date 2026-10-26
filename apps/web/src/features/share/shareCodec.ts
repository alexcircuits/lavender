import type { FeedId } from '../../contracts';
import type { CameraPose, EntityRef, EntitySource, TrackingMode } from '../../globe';

export interface ShareState {
  readonly camera?: CameraPose | null | undefined;
  readonly enabledLayers?: readonly FeedId[] | undefined;
  readonly visualPreset?: 'normal' | 'noir' | 'nvg' | 'flir' | 'thermal' | 'crt' | undefined;
  readonly hudEnabled?: boolean | undefined;
  readonly scopeMaskEnabled?: boolean | undefined;
  readonly sonarEnabled?: boolean | undefined;
  readonly selection?: EntityRef | null | undefined;
  readonly trackingMode?: TrackingMode | undefined;
  readonly imageryId?: string | null | undefined;
}

export interface DecodedShareState {
  readonly version: number;
  readonly camera: {
    readonly lat: number;
    readonly lon: number;
    readonly altM: number;
    readonly headingDeg: number;
    readonly pitchDeg: number;
    readonly rollDeg: number;
  } | null;
  readonly enabledLayers: readonly FeedId[];
  readonly visualPreset: 'normal' | 'noir' | 'thermal' | 'nvg' | 'crt';
  readonly hudEnabled: boolean;
  readonly scopeMaskEnabled: boolean;
  readonly sonarEnabled: boolean;
  readonly selection: EntityRef | null;
  readonly trackingMode: TrackingMode;
  readonly imageryId: string | null;
}

export const FEED_TO_TOKEN: Readonly<Record<FeedId, string>> = Object.freeze({
  aircraft: 'f',
  vessels: 'a',
  satellites: 's',
  earthquakes: 'e',
  cyclones: 'y',
  fires: 'w',
  perimeters: '2',
  launches: 'x',
  bikeshare: 'b',
  transit: 'j',
});

export const TOKEN_TO_FEED: Readonly<Record<string, FeedId>> = Object.freeze({
  f: 'aircraft',
  m: 'aircraft', // legacy military
  a: 'vessels',
  s: 'satellites',
  e: 'earthquakes',
  y: 'cyclones',
  w: 'fires',
  2: 'perimeters',
  x: 'launches',
  b: 'bikeshare',
  j: 'transit',
});

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

function parseFiniteFloat(value: string | null): number | null {
  if (value === null) return null;
  const num = Number.parseFloat(value.trim());
  return Number.isFinite(num) ? num : null;
}

/**
 * Encodes the given workspace state into a shareable URL hash.
 */
export function encodeShareHash(state: ShareState): string {
  const params = new URLSearchParams();
  params.set('v', '2');

  if (state.camera) {
    const lat = clamp(state.camera.lat, -90, 90);
    const lon = ((((state.camera.lon + 180) % 360) + 360) % 360) - 180;
    params.set('lat', lat.toFixed(4));
    params.set('lon', lon.toFixed(4));
    params.set('alt', Math.round(state.camera.heightM).toString());

    const hdgDeg = Math.round(((((state.camera.headingRad * 180) / Math.PI) % 360) + 360) % 360);
    params.set('heading', hdgDeg.toString());

    const pitchDeg = Math.round(clamp((state.camera.pitchRad * 180) / Math.PI, -90, 90));
    params.set('pitch', pitchDeg.toString());

    const rollDeg = Math.round(clamp((state.camera.rollRad * 180) / Math.PI, -180, 180));
    if (rollDeg !== 0) {
      params.set('roll', rollDeg.toString());
    }
  }

  if (state.enabledLayers && state.enabledLayers.length > 0) {
    const tokens = state.enabledLayers
      .map((f) => FEED_TO_TOKEN[f])
      .filter((t): t is string => Boolean(t))
      .sort()
      .join('');
    if (tokens.length > 0) {
      params.set('l', tokens);
    }
  }

  if (state.visualPreset && state.visualPreset !== 'normal') {
    params.set('style', state.visualPreset === 'thermal' ? 'flir' : state.visualPreset);
  }

  if (state.hudEnabled) {
    params.set('hud', '1');
  }
  if (state.scopeMaskEnabled) {
    params.set('sm', '1');
  }
  if (state.sonarEnabled) {
    params.set('sonar', '1');
  }

  if (state.selection) {
    params.set('sel', `${state.selection.feed}:${state.selection.id}`);
    if (state.trackingMode === 'cockpit') {
      params.set('mode', 'cockpit');
    } else if (state.trackingMode === 'follow') {
      params.set('mode', 'follow');
    }
  }

  if (state.imageryId && state.imageryId !== 'default') {
    params.set('map', state.imageryId);
  }

  return params.toString();
}

/**
 * Builds a complete shareable URL from the given ShareState, stripping any ?lang override
 * to ensure the recipient's locale preference is preserved.
 */
export function encodeShareUrl(state: ShareState, baseHref?: string): string {
  const currentHref =
    baseHref ??
    (typeof window !== 'undefined' ? window.location.href : 'https://lavender.noir.systems/');
  const url = new URL(currentHref);
  url.searchParams.delete('lang');
  url.hash = encodeShareHash(state);
  return url.toString();
}

/**
 * Parses and validates a URL hash string (modern v=2 or legacy GEV format).
 * Returns null if the hash does not contain usable coordinates or valid versioning.
 */
export function decodeShareHash(rawHash: string): DecodedShareState | null {
  const cleaned = rawHash.startsWith('#') ? rawHash.slice(1) : rawHash;
  if (!cleaned.trim()) return null;

  const params = new URLSearchParams(cleaned);

  const rawLat = parseFiniteFloat(params.get('lat'));
  const rawLon = parseFiniteFloat(params.get('lon'));
  if (rawLat === null || rawLon === null) return null;

  const lat = clamp(rawLat, -90, 90);
  const lon = ((((rawLon + 180) % 360) + 360) % 360) - 180;
  const altM = Math.max(10, parseFiniteFloat(params.get('alt')) ?? 10_000_000);
  const headingDeg = parseFiniteFloat(params.get('heading')) ?? 0;
  const pitchDeg = clamp(parseFiniteFloat(params.get('pitch')) ?? -90, -90, 90);
  const rollDeg = clamp(parseFiniteFloat(params.get('roll')) ?? 0, -180, 180);

  const rawStyle = params.get('style')?.toLowerCase();
  let visualPreset: 'normal' | 'noir' | 'thermal' | 'nvg' | 'crt' = 'normal';
  if (rawStyle === 'noir' || rawStyle === 'nvg' || rawStyle === 'crt') {
    visualPreset = rawStyle;
  } else if (rawStyle === 'thermal' || rawStyle === 'flir') {
    visualPreset = 'thermal';
  }

  const hudEnabled = params.get('hud') === '1' || params.get('hv') === '1';
  const scopeMaskEnabled = params.get('sm') === '1' || params.get('sc') === '1';
  const sonarEnabled = params.get('sonar') === '1';

  // Decode enabled layers from compact token string
  const rawLayers = params.get('l');
  const enabledLayers: FeedId[] = [];
  if (rawLayers) {
    const seen = new Set<FeedId>();
    for (const char of rawLayers) {
      const feed = TOKEN_TO_FEED[char];
      if (feed && !seen.has(feed)) {
        seen.add(feed);
        enabledLayers.push(feed);
      }
    }
  }

  // Decode selection
  let selection: EntityRef | null = null;
  const rawSel = params.get('sel');
  if (rawSel?.includes(':')) {
    const [feedStr, ...idParts] = rawSel.split(':');
    const id = idParts.join(':');
    if (feedStr && id) {
      selection = { feed: feedStr as EntitySource, id };
    }
  } else if (params.get('track')) {
    // Legacy GEV tracking ID
    const trackId = params.get('track');
    if (trackId) {
      selection = { feed: 'aircraft', id: trackId };
    }
  }

  const rawMode = params.get('mode');
  const trackingMode: TrackingMode = rawMode === 'cockpit' ? 'cockpit' : 'follow';
  const imageryId = params.get('map') ?? null;
  const version = params.get('v') === '2' ? 2 : 1;

  return {
    version,
    camera: {
      lat,
      lon,
      altM,
      headingDeg,
      pitchDeg,
      rollDeg,
    },
    enabledLayers,
    visualPreset,
    hudEnabled,
    scopeMaskEnabled,
    sonarEnabled,
    selection,
    trackingMode,
    imageryId,
  };
}

/**
 * Copies the generated share link URL to clipboard.
 */
export async function copyShareUrl(state: ShareState): Promise<boolean> {
  const url = encodeShareUrl(state);
  try {
    if (typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(url);
      return true;
    }
  } catch (error) {
    console.warn('Failed to copy to clipboard', error);
  }
  return false;
}
