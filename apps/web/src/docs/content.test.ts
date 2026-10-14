import { beforeAll, describe, expect, it } from 'vitest';
import {
  docById,
  docHref,
  documents,
  extractHeadings,
  loadDoc,
  loadSearchIndex,
  parseRoute,
  resolveImage,
  resolveLink,
  searchDocs,
  sourceFiles,
} from './content';

beforeAll(async () => {
  expect(await loadSearchIndex()).toBe(true);
});

describe('repository documentation', () => {
  it('loads every guide from Markdown with a unique stable address', async () => {
    expect(documents.length).toBeGreaterThan(35);
    expect(new Set(documents.map((doc) => doc.id)).size).toBe(documents.length);
    for (const doc of documents) {
      expect(doc.loaded).toBe(true);
      expect(doc.markdown).toMatch(/^# /);
      expect(doc.body).not.toMatch(/^# /);
      expect(doc.headings.every((heading) => heading.id.length > 0)).toBe(true);
      expect(await loadDoc(doc.id)).toBe(doc);
    }
  });

  it('round-trips pages, sections, Unicode and malformed addresses', () => {
    expect(parseRoute('')).toEqual({ id: 'overview', section: '' });
    expect(parseRoute(docHref('architecture', 'a-record-end-to-end'))).toEqual({
      id: 'architecture',
      section: 'a-record-end-to-end',
    });
    expect(parseRoute(docHref('glossary', 'Координати'))).toEqual({
      id: 'glossary',
      section: 'Координати',
    });
    expect(parseRoute('#/%E0%A4%A')).toEqual({ id: 'missing', section: '' });
  });

  it('matches rendered heading slugs, preserving identifiers and ignoring fenced code', () => {
    expect(
      extractHeadings(
        '## `from_env`\n## Repeat\n```md\n## Repeat\n```\n#### Repeat\n## Repeat\n### Привіт',
      ),
    ).toEqual([
      { id: 'from_env', text: 'from_env', level: 2 },
      { id: 'repeat', text: 'Repeat', level: 2 },
      { id: 'repeat-2', text: 'Repeat', level: 2 },
      { id: 'привіт', text: 'Привіт', level: 3 },
    ]);
  });

  it('resolves links to current guides and current source files', async () => {
    const doc = await loadDoc('architecture');
    expect(resolveLink(doc, 'getting-started.md#run-with-hot-reload')).toEqual({
      href: '#/getting-started#run-with-hot-reload',
    });
    expect(resolveLink(doc, '../../crates/lavender-core/src/hub/slots.rs').source).toBe(
      'crates/lavender-core/src/hub/slots.rs',
    );
    expect(sourceFiles.has('apps/desktop/src-tauri/src/commands.rs')).toBe(true);
    expect(sourceFiles.has('apps/web/src/feeds/FeedController.ts')).toBe(true);
    const source = await sourceFiles.get('crates/lavender-core/src/hub/slots.rs')?.();
    expect(source).toContain('impl RegionBudget');
    expect(resolveLink(doc, '../adr/').href).toMatch(/^#\/adr\/0001/);
    expect(resolveLink(doc, 'javascript:alert(1)').href).toBe('');
    expect(resolveLink(doc, '//example.com').href).toBe('');
  });

  it('searches real guide bodies, ranks titles and supplies readable snippets', () => {
    expect(searchDocs('fixtures').some(({ doc }) => doc.id === 'getting-started')).toBe(true);
    expect(searchDocs('Rust rewrite')[0]?.doc.id).toBe('rust-rewrite');
    expect(
      searchDocs('cancellation').some(({ snippet }) =>
        snippet.toLowerCase().includes('cancellation'),
      ),
    ).toBe(true);
    expect(searchDocs('zzzzzzunfindable')).toEqual([]);
    expect(
      searchDocs('data licensing').some(({ doc }) => doc.id === 'audit/data-sources-and-licensing'),
    ).toBe(true);
  });

  it('serves checked-in evidence images from the same build', () => {
    const doc = docById.get('architecture/spikes/2026-10-11-tauri-cesium-macos');
    if (!doc) throw new Error('Spike guide missing');
    expect(resolveImage(doc, '2026-10-11-tauri-cesium-macos.png')).toContain(
      '2026-10-11-tauri-cesium-macos',
    );
    expect(resolveImage(doc, 'https://example.com/image.png')).toBeUndefined();
  });
});
