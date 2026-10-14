import type { FeedFilters } from '../contracts';
import { type MapAnnotation, validateAnnotations } from '../features/annotations/model';
import type { CameraPose } from '../globe';
import { MAX_HEIGHT_M, MIN_HEIGHT_M } from '../globe/camera';

export const WORKSPACE_SCHEMA_VERSION = 2;
export const MAX_DOCUMENT_BYTES = 5_500;
export const MAX_VIEW_PAYLOAD_LENGTH = 7_500;
export const MAX_SHARE_URL_LENGTH = 8_192;

export interface WorkspaceLayer {
  /** A string so links from a newer registry can safely ignore unfamiliar layers. */
  readonly id: string;
  readonly enabled: boolean;
  readonly options: FeedFilters;
}

export interface WorkspaceDocument {
  readonly schemaVersion: typeof WORKSPACE_SCHEMA_VERSION;
  readonly camera: CameraPose;
  readonly layers: readonly WorkspaceLayer[];
  readonly imageryId: string | null;
  readonly selection: { readonly feed: string; readonly id: string } | null;
  readonly annotations: readonly MapAnnotation[];
}

export class WorkspaceLinkError extends Error {
  constructor(message = 'This link is invalid or too large.') {
    super(message);
    this.name = 'WorkspaceLinkError';
  }
}

function record(value: unknown, fields: readonly string[]): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value))
    throw new WorkspaceLinkError();
  if (Object.keys(value).some((key) => !fields.includes(key))) throw new WorkspaceLinkError();
  return value as Record<string, unknown>;
}

function number(value: unknown, min: number, max: number): number {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < min || value > max)
    throw new WorkspaceLinkError();
  return value;
}

function text(value: unknown, max: number): string {
  if (
    typeof value !== 'string' ||
    value.length === 0 ||
    value.length > max ||
    /\p{Cc}/u.test(value)
  )
    throw new WorkspaceLinkError();
  return value;
}

export function validateCameraPose(value: unknown): CameraPose {
  const c = record(value, ['lon', 'lat', 'heightM', 'headingRad', 'pitchRad', 'rollRad']);
  return {
    lon: number(c.lon, -180, 180),
    lat: number(c.lat, -90, 90),
    heightM: number(c.heightM, MIN_HEIGHT_M, MAX_HEIGHT_M),
    headingRad: number(c.headingRad, 0, 2 * Math.PI),
    pitchRad: number(c.pitchRad, -Math.PI / 2, Math.PI / 2),
    rollRad: number(c.rollRad, -2 * Math.PI, 2 * Math.PI),
  };
}

function optionsFor(value: unknown, id: string): FeedFilters {
  const allowed =
    id === 'earthquakes'
      ? ['minMagnitude']
      : id === 'aircraft'
        ? ['militaryOnly']
        : id === 'satellites'
          ? ['groups']
          : ['minMagnitude', 'militaryOnly', 'groups'];
  const options = record(value, allowed);
  const result: FeedFilters = {};
  if (options.minMagnitude !== undefined)
    result.minMagnitude = number(options.minMagnitude, -2, 10);
  if (options.militaryOnly !== undefined) {
    if (typeof options.militaryOnly !== 'boolean') throw new WorkspaceLinkError();
    result.militaryOnly = options.militaryOnly;
  }
  if (options.groups !== undefined) {
    if (!Array.isArray(options.groups) || options.groups.length > 32)
      throw new WorkspaceLinkError();
    result.groups = options.groups.map((group) => {
      const name = text(group, 32);
      if (!/^[a-z0-9-]+$/.test(name)) throw new WorkspaceLinkError();
      return name;
    });
  }
  return result;
}

/** v1 reserved an empty annotation array; v2 persists validated resolved geometry. */
export function migrateWorkspaceDocument(value: unknown): WorkspaceDocument {
  const doc = record(value, [
    'schemaVersion',
    'camera',
    'layers',
    'imageryId',
    'selection',
    'annotations',
  ]);
  if (doc.schemaVersion !== 1 && doc.schemaVersion !== WORKSPACE_SCHEMA_VERSION)
    throw new WorkspaceLinkError('This workspace version is not supported.');
  if (!Array.isArray(doc.layers) || doc.layers.length > 32) throw new WorkspaceLinkError();
  const seen = new Set<string>();
  const layers = doc.layers.map((value): WorkspaceLayer => {
    const layer = record(value, ['id', 'enabled', 'options']);
    const id = text(layer.id, 64);
    if (!/^[a-z][a-z0-9-]*$/.test(id) || seen.has(id) || typeof layer.enabled !== 'boolean')
      throw new WorkspaceLinkError();
    seen.add(id);
    return { id, enabled: layer.enabled, options: optionsFor(layer.options, id) };
  });
  let selection: WorkspaceDocument['selection'] = null;
  if (doc.selection !== null) {
    const ref = record(doc.selection, ['feed', 'id']);
    selection = { feed: text(ref.feed, 64), id: text(ref.id, 256) };
  }
  let annotations: readonly MapAnnotation[];
  try {
    if (
      doc.schemaVersion === 1 &&
      (!Array.isArray(doc.annotations) || doc.annotations.length !== 0)
    )
      throw new WorkspaceLinkError();
    annotations = validateAnnotations(doc.annotations);
  } catch {
    throw new WorkspaceLinkError('This view contains invalid or unsupported annotations.');
  }
  return {
    schemaVersion: WORKSPACE_SCHEMA_VERSION,
    camera: validateCameraPose(doc.camera),
    layers,
    imageryId: doc.imageryId === null ? null : text(doc.imageryId, 128),
    selection,
    annotations,
  };
}

/** UTF-8 (including Unicode entity ids), minified JSON and URL-safe base64 without padding. */
export function encodeWorkspaceDocument(document: WorkspaceDocument): string {
  const bytes = new TextEncoder().encode(JSON.stringify(migrateWorkspaceDocument(document)));
  if (bytes.length > MAX_DOCUMENT_BYTES)
    throw new WorkspaceLinkError('This view is too large to share.');
  const encoded = btoa(String.fromCharCode(...bytes))
    .replaceAll('+', '-')
    .replaceAll('/', '_')
    .replace(/=+$/, '');
  if (encoded.length > MAX_VIEW_PAYLOAD_LENGTH) throw new WorkspaceLinkError();
  return encoded;
}

export function decodeWorkspaceDocument(payload: string): WorkspaceDocument {
  if (
    payload.length === 0 ||
    payload.length > MAX_VIEW_PAYLOAD_LENGTH ||
    !/^[A-Za-z0-9_-]+$/.test(payload)
  )
    throw new WorkspaceLinkError();
  try {
    const binary = atob(payload.replaceAll('-', '+').replaceAll('_', '/'));
    if (binary.length > MAX_DOCUMENT_BYTES) throw new WorkspaceLinkError();
    const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
    const json = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
    return migrateWorkspaceDocument(JSON.parse(json));
  } catch (error) {
    if (error instanceof WorkspaceLinkError) throw error;
    throw new WorkspaceLinkError();
  }
}

export function workspaceDocumentFromUrl(url: string): WorkspaceDocument | null {
  if (url.length > MAX_SHARE_URL_LENGTH) throw new WorkspaceLinkError();
  const parsed = new URL(url);
  const native = parsed.protocol === 'lavender:' && parsed.hostname === 'view';
  const parameters = new URLSearchParams(native ? parsed.search : parsed.hash.slice(1));
  if (!parameters.has('view')) return null;
  if (
    Array.from(parameters.keys()).some((key) => key !== 'view') ||
    parameters.getAll('view').length !== 1
  )
    throw new WorkspaceLinkError();
  return decodeWorkspaceDocument(parameters.get('view') ?? '');
}

export function workspaceShareUrl(document: WorkspaceDocument, baseUrl: string): string {
  const url = new URL(baseUrl);
  if (!['http:', 'https:'].includes(url.protocol))
    throw new WorkspaceLinkError('Sharing is not available in this desktop prototype.');
  url.hash = `view=${encodeWorkspaceDocument(document)}`;
  if (url.href.length > MAX_SHARE_URL_LENGTH) throw new WorkspaceLinkError();
  return url.href;
}

/** Reserved transport format only. No native OS protocol handler is registered by this app. */
export function reservedNativeWorkspaceUrl(document: WorkspaceDocument): string {
  return `lavender://view?view=${encodeWorkspaceDocument(document)}`;
}
