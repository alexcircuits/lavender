/**
 * Map symbology tokens (docs/design/design-system.md §4), read from the computed CSS at mount
 * so the globe follows tokens.css. Every token has its validated value as a fallback: the
 * engine must still draw when a token is missing or (in forced-colors mode) a system colour
 * keyword the WebGL side cannot use.
 */

export interface MapPalette {
  readonly air: string;
  readonly maritime: string;
  readonly space: string;
  readonly selection: string;
  readonly outline: string;
  /** Light neutral keyline just inside the outline of hazard symbols (§4). */
  readonly keyline: string;
  readonly ink: string;
  /** Boundary classes (Natural Earth): distinct line style + colour per class. */
  readonly boundaryIntl: string;
  readonly boundaryDisputed: string;
  readonly boundaryBreakaway: string;
  readonly boundaryMaritime: string;
  /** Named geofence regions: outlined polygons. */
  readonly geofence: string;
  /** Hazard magnitude ramp, low → high (five ordinal steps). */
  readonly hazardRamp: readonly [string, string, string, string, string];
  /** CSS font shorthand for globe labels (Geist 500 12 px). */
  readonly labelFont: string;
}

export const DEFAULT_FONT_SANS = "'Geist Variable', 'Geist', ui-sans-serif, system-ui, sans-serif";

export const DEFAULT_PALETTE: MapPalette = {
  air: '#3987e5',
  maritime: '#199e70',
  space: '#d6d3e6',
  selection: '#b9a6ff',
  outline: '#0a0a0b',
  keyline: '#bcbcc4',
  ink: '#fafafa',
  boundaryIntl: '#e2e8f0',
  boundaryDisputed: '#f59e0b',
  boundaryBreakaway: '#ec4899',
  boundaryMaritime: '#38bdf8',
  geofence: '#fde047',
  hazardRamp: ['#8f3c19', '#b9471d', '#d95926', '#ef7a45', '#f8a57a'],
  labelFont: `500 12px ${DEFAULT_FONT_SANS}`,
};

/** Returns a custom property's computed value, or '' when it is not set. */
export type TokenReader = (name: string) => string;

export function cssTokenReader(element: Element = document.documentElement): TokenReader {
  const style = getComputedStyle(element);
  return (name) => style.getPropertyValue(name).trim();
}

function token(read: TokenReader, name: string, fallback: string): string {
  return read(name) || fallback;
}

export function readPalette(read: TokenReader): MapPalette {
  const ramp = DEFAULT_PALETTE.hazardRamp;
  return {
    air: token(read, '--map-air', DEFAULT_PALETTE.air),
    maritime: token(read, '--map-maritime', DEFAULT_PALETTE.maritime),
    space: token(read, '--map-space', DEFAULT_PALETTE.space),
    selection: token(read, '--map-selection', DEFAULT_PALETTE.selection),
    outline: token(read, '--map-outline', DEFAULT_PALETTE.outline),
    keyline: token(read, '--map-keyline', DEFAULT_PALETTE.keyline),
    ink: token(read, '--ink', DEFAULT_PALETTE.ink),
    boundaryIntl: token(read, '--map-boundary-intl', DEFAULT_PALETTE.boundaryIntl),
    boundaryDisputed: token(read, '--map-boundary-disputed', DEFAULT_PALETTE.boundaryDisputed),
    boundaryBreakaway: token(read, '--map-boundary-breakaway', DEFAULT_PALETTE.boundaryBreakaway),
    boundaryMaritime: token(read, '--map-boundary-maritime', DEFAULT_PALETTE.boundaryMaritime),
    geofence: token(read, '--map-geofence', DEFAULT_PALETTE.geofence),
    hazardRamp: [
      token(read, '--hazard-1', ramp[0]),
      token(read, '--hazard-2', ramp[1]),
      token(read, '--hazard-3', ramp[2]),
      token(read, '--hazard-4', ramp[3]),
      token(read, '--hazard-5', ramp[4]),
    ],
    labelFont: `500 12px ${token(read, '--font-sans', DEFAULT_FONT_SANS)}`,
  };
}
