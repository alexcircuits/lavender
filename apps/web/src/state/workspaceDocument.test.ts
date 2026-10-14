import { describe, expect, it } from 'vitest';
import { pin } from '../test/annotation';
import { document } from '../test/workspaceDocument';
import {
  decodeWorkspaceDocument,
  encodeWorkspaceDocument,
  MAX_DOCUMENT_BYTES,
  MAX_SHARE_URL_LENGTH,
  MAX_VIEW_PAYLOAD_LENGTH,
  migrateWorkspaceDocument,
  reservedNativeWorkspaceUrl,
  workspaceDocumentFromUrl,
  workspaceShareUrl,
} from './workspaceDocument';

const rawPayload = (value: unknown) =>
  btoa(String.fromCharCode(...new TextEncoder().encode(JSON.stringify(value))))
    .replaceAll('+', '-')
    .replaceAll('/', '_')
    .replace(/=+$/, '');

describe('workspace document v2', () => {
  it('round-trips Unicode and all view fields through a bounded URL-safe payload', () => {
    const payload = encodeWorkspaceDocument(document);
    expect(payload).toMatch(/^[A-Za-z0-9_-]+$/);
    expect(payload.length).toBeLessThan(MAX_VIEW_PAYLOAD_LENGTH);
    expect(decodeWorkspaceDocument(payload)).toEqual(document);
    const url = workspaceShareUrl(document, 'https://lavender.example/work?mode=fixture#docs');
    expect(workspaceDocumentFromUrl(url)).toEqual(document);
    expect(workspaceDocumentFromUrl(reservedNativeWorkspaceUrl(document))).toEqual(document);
    expect(workspaceDocumentFromUrl('https://lavender.example/#docs')).toBeNull();
  });

  it('migrates old empty v1 links and round-trips pin, line and area annotations', () => {
    expect(migrateWorkspaceDocument({ ...document, schemaVersion: 1 })).toEqual(document);
    const annotations = [
      pin,
      {
        ...pin,
        id: 'line',
        kind: 'line' as const,
        points: [
          { lat: 0, lon: 179 },
          { lat: 0, lon: -179 },
        ],
      },
      {
        ...pin,
        id: 'area',
        kind: 'area' as const,
        points: [
          { lat: 0, lon: 0 },
          { lat: 0, lon: 1 },
          { lat: 1, lon: 0 },
        ],
      },
    ];
    const view = { ...document, annotations };
    expect(decodeWorkspaceDocument(encodeWorkspaceDocument(view))).toEqual(view);
    expect(() => migrateWorkspaceDocument({ ...view, schemaVersion: 1 })).toThrow();
  });
  it('preserves new cyclone layers and entity references for catalog authorization', () => {
    const view = {
      ...document,
      layers: [{ id: 'cyclones', enabled: true, options: {} }],
      selection: { feed: 'cyclones', id: 'al092026' },
    };
    expect(decodeWorkspaceDocument(encodeWorkspaceDocument(view))).toEqual(view);
  });
  it('keeps annotation size failures explicit without truncating notes or geometry', () => {
    const view = {
      ...document,
      annotations: Array.from({ length: 24 }, (_, i) => ({
        ...pin,
        id: `mark-${i}`,
        note: 'x'.repeat(500),
      })),
    };
    expect(() => encodeWorkspaceDocument(view)).toThrow(/large/);
    expect(view.annotations).toHaveLength(24);
  });

  it.each(['', '!', 'a', 'AAAA', '%%%%', 'a'.repeat(MAX_VIEW_PAYLOAD_LENGTH + 1)])(
    'rejects malformed or oversized payloads',
    (payload) => {
      expect(() => decodeWorkspaceDocument(payload)).toThrow();
    },
  );

  it('bounds decoded bytes, documents and complete URLs', () => {
    expect(() => decodeWorkspaceDocument(btoa(' '.repeat(MAX_DOCUMENT_BYTES + 1)))).toThrow();
    const large = {
      ...document,
      layers: Array.from({ length: 32 }, (_, index) => ({
        id: `unknown-${index}`,
        enabled: true,
        options: { groups: Array.from({ length: 32 }, () => 'x'.repeat(32)) },
      })),
    };
    expect(() => encodeWorkspaceDocument(large)).toThrow(/large/);
    expect(() =>
      workspaceShareUrl(document, `https://lavender.example/${'x'.repeat(MAX_SHARE_URL_LENGTH)}`),
    ).toThrow();
    expect(() =>
      workspaceDocumentFromUrl(
        `https://lavender.example/#view=${'a'.repeat(MAX_SHARE_URL_LENGTH)}`,
      ),
    ).toThrow();
  });

  it.each([
    { ...document, schemaVersion: 3 },
    { ...document, extra: true },
    { ...document, camera: { ...document.camera, extra: 1 } },
    { ...document, selection: { ...document.selection, url: 'https://evil.example' } },
    { ...document, annotations: [{ id: 'unimplemented' }] },
    { ...document, layers: [{ id: 'aircraft', enabled: true, options: { groups: ['stations'] } }] },
    { ...document, layers: [{ id: 'earthquakes', enabled: true, options: { minMagnitude: 99 } }] },
    { ...document, layers: [{ id: 'aircraft', enabled: true, options: { militaryOnly: 'true' } }] },
    {
      ...document,
      layers: [{ id: 'satellites', enabled: true, options: { groups: ['Bad group'] } }],
    },
    { ...document, layers: [document.layers[0], document.layers[0]] },
  ])('rejects unknown fields, versions, unsupported annotations and invalid options', (value) => {
    expect(() => decodeWorkspaceDocument(rawPayload(value))).toThrow();
  });

  it.each([
    ['lon', 181],
    ['lat', -91],
    ['heightM', 99],
    ['heightM', 40_000_001],
    ['headingRad', -1],
    ['pitchRad', 2],
    ['rollRad', 7],
    ['lon', Number.NaN],
    ['heightM', Number.POSITIVE_INFINITY],
  ])('rejects nonfinite/out-of-range camera %s=%s', (key, value) => {
    expect(() =>
      migrateWorkspaceDocument({ ...document, camera: { ...document.camera, [key]: value } }),
    ).toThrow();
  });

  it('rejects nested prototype fields and ambiguous URL parameters', () => {
    const dangerous = JSON.parse(
      JSON.stringify(document).replace('"options":{', '"options":{"__proto__":{},'),
    );
    expect(() => migrateWorkspaceDocument(dangerous)).toThrow();
    const encoded = encodeWorkspaceDocument(document);
    expect(() =>
      workspaceDocumentFromUrl(`https://lavender.example/#view=${encoded}&view=${encoded}`),
    ).toThrow();
    expect(() =>
      workspaceDocumentFromUrl(`https://lavender.example/#view=${encoded}&other=1`),
    ).toThrow();
  });
});
