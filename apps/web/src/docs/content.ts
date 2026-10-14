import GithubSlugger from 'github-slugger';

export interface Heading {
  id: string;
  text: string;
  level: number;
}

export interface Doc {
  id: string;
  path: string;
  title: string;
  group: string;
  markdown: string;
  body: string;
  headings: Heading[];
  searchable: string;
  loaded: boolean;
}

export function sourceLanguage(path: string): string {
  const extension = path.split('.').at(-1) ?? 'text';
  return (
    (
      { rs: 'rust', ts: 'typescript', tsx: 'tsx', sh: 'bash', md: 'markdown' } as Record<
        string,
        string
      >
    )[extension] ?? extension
  );
}

/** Repository Markdown is the only content source. Vite bundles it at build time. */
const markdownFiles = import.meta.glob<string>('../../../../docs/**/*.md', {
  query: '?raw',
  import: 'default',
});

const guides = [
  ['overview', 'docs/README.md', 'Overview', 'Get started'],
  ['start-here', 'docs/development/start-here.md', 'Start here', 'Get started'],
  ['getting-started', 'docs/development/getting-started.md', 'Run locally', 'Get started'],
  ['architecture', 'docs/development/architecture.md', 'Architecture', 'Understand Lavender'],
  ['codebase-map', 'docs/development/codebase-map.md', 'Codebase map', 'Understand Lavender'],
  ['rust-rewrite', 'docs/development/rust-rewrite.md', 'Rust rewrite', 'Understand Lavender'],
  ['add-provider', 'docs/development/add-provider.md', 'Add a data provider', 'Build & contribute'],
  ['web-and-desktop', 'docs/development/web-and-desktop.md', 'Web & desktop', 'Build & contribute'],
  ['testing', 'docs/development/testing.md', 'Testing & debugging', 'Build & contribute'],
  [
    'contributing',
    'docs/development/contributing.md',
    'Contribution workflow',
    'Build & contribute',
  ],
  ['reference', 'docs/development/reference.md', 'Functions & modules', 'Reference'],
  ['glossary', 'docs/development/glossary.md', 'Glossary', 'Reference'],
  ['roadmap', 'docs/roadmap.md', 'Roadmap', 'Project'],
  ['task-index', 'docs/development/task-index.md', 'Task index', 'Project'],
  ['verification', 'docs/development/verification.md', 'Verification', 'Project'],
] as const;

export function plainText(markdown: string): string {
  return markdown
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/(?<!\w)_([^_\n]+)_(?!\w)/g, '$1')
    .replace(/[`*#>|]/g, '')
    .replace(/\s+/g, ' ')
    .trim();
}

/** Use GitHub's slug rules, as rehype-slug does; fenced code is never an outline entry. */
export function extractHeadings(markdown: string): Heading[] {
  const slugger = new GithubSlugger();
  const headings: Heading[] = [];
  let fence = '';
  for (const line of markdown.split('\n')) {
    const marker = line.match(/^\s{0,3}(`{3,}|~{3,})/);
    if (marker?.[1]) {
      if (!fence) fence = marker[1];
      else if (marker[1][0] === fence[0] && marker[1].length >= fence.length) fence = '';
      continue;
    }
    if (fence) continue;
    const match = line.match(/^(#{1,6})\s+(.+?)(?:\s+#+)?$/);
    if (!match?.[1] || !match[2]) continue;
    const text = plainText(match[2]);
    const id = slugger.slug(text);
    if (match[1].length >= 2 && match[1].length <= 3)
      headings.push({ id, text, level: match[1].length });
  }
  return headings;
}

const navigationTitles: Record<string, string> = {
  'adr/0001-repository-layout-and-legacy-retention': '0001 · Repository layout',
  'adr/0002-shared-rust-core-boundary': '0002 · Shared Rust core',
  'adr/0003-cesiumjs-engine-without-widgets': '0003 · Cesium without widgets',
  'adr/0004-frontend-stack': '0004 · Frontend stack',
  'adr/0005-platform-abstraction-and-realtime-transport': '0005 · Platform & transport',
  'adr/0006-storage-secrets-and-configuration': '0006 · Storage & secrets',
  'adr/0007-generated-type-contracts': '0007 · Generated contracts',
  'adr/0008-data-source-governance-and-network-egress': '0008 · Sources & egress',
  'architecture/overview': 'Architecture specification',
  'architecture/rust-migration': 'Original migration plan',
  'architecture/frontend': 'Frontend specification',
  'architecture/security': 'Security specification',
  'architecture/tauri': 'Desktop specification',
  'architecture/spikes/2026-10-11-tauri-cesium-macos': 'macOS WebView spike',
  'audit/data-sources-and-licensing': 'Sources & licensing',
  'audit/design-references': 'Design references',
  'audit/feature-parity-matrix': 'Feature parity matrix',
  'audit/upstream-backend': 'Upstream backend',
  'audit/upstream-layers': 'Upstream layers',
  'audit/upstream-ui-and-systems': 'Upstream interface',
  'design/design-system': 'Design system',
  'design/documentation': 'Documentation design',
  'performance/specification': 'Performance specification',
  'product/specification': 'Product specification',
  'research/2026-10-technology-research': 'Technology research',
  progress: 'Progress log',
  'roadmap-detail': 'Original task breakdown',
};

const entries = Object.keys(markdownFiles).map((key) => {
  const path = key.replace('../../../../', '');
  const guide = guides.find((entry) => entry[1] === path);
  const slug = path
    .replace(/^docs\//, '')
    .replace(/\.md$/, '')
    .toLowerCase();
  const title = navigationTitles[slug] ?? slug.split('/').at(-1)?.replace(/-/g, ' ') ?? slug;
  const groupNames: Record<string, string> = {
    architecture: 'Architecture specifications',
    adr: 'Architecture decisions',
    audit: 'Upstream audits',
    design: 'Design',
    product: 'Product specifications',
    performance: 'Performance',
    research: 'Research',
  };
  return {
    id: guide?.[0] ?? slug,
    path,
    title: guide?.[2] ?? title,
    group: guide?.[3] ?? groupNames[path.split('/')[1] ?? ''] ?? 'History',
    markdown: '',
    body: '',
    headings: [],
    searchable: '',
    loaded: false,
  } satisfies Doc;
});

export const documents: Doc[] = [
  ...guides.flatMap(([id]) => entries.filter((doc) => doc.id === id)),
  ...entries
    .filter((doc) => !guides.some(([id]) => doc.id === id))
    .sort((a, b) => a.path.localeCompare(b.path)),
];
export const mainGuides = documents.slice(0, guides.length);
export const advancedGuides = documents.slice(guides.length);
export const docById = new Map(documents.map((doc) => [doc.id, doc]));
const docByPath = new Map(documents.map((doc) => [doc.path, doc]));

const pending = new Map<string, Promise<Doc>>();
export function loadDoc(id: string): Promise<Doc> {
  const doc = docById.get(id);
  const read = doc ? markdownFiles[`../../../../${doc.path}`] : undefined;
  if (!doc || !read) return Promise.reject(new Error('Guide not found'));
  if (doc.loaded) return Promise.resolve(doc);
  const existing = pending.get(id);
  if (existing) return existing;
  const promise = read()
    .then((markdown) => {
      const body = markdown.replace(/^# [^\n]+\n+/, '');
      Object.assign(doc, {
        markdown,
        body,
        headings: extractHeadings(body),
        searchable: plainText(body),
        loaded: true,
      });
      pending.delete(id);
      return doc;
    })
    .catch((error: unknown) => {
      pending.delete(id);
      throw error;
    });
  pending.set(id, promise);
  return promise;
}

/** Full-text search loads the archive once, on demand; opening docs stays small. */
export async function loadSearchIndex(): Promise<boolean> {
  const results = await Promise.allSettled(documents.map((doc) => loadDoc(doc.id)));
  return results.every((result) => result.status === 'fulfilled');
}

export function docHref(id: string, section = ''): string {
  return `#/${id}${section ? `#${encodeURIComponent(section)}` : ''}`;
}

export function parseRoute(hash: string): { id: string; section: string } {
  const [id = '', section = ''] = hash.replace(/^#\/?/, '').split('#');
  try {
    return { id: decodeURIComponent(id) || 'overview', section: decodeURIComponent(section) };
  } catch {
    return { id: 'missing', section: '' };
  }
}

/** Raw code is lazy: following a source link reads this checkout, including uncommitted modules. */
const rawFiles = import.meta.glob<string>(
  [
    '../../../../crates/**/*.{rs,toml}',
    '../../../desktop/src-tauri/**/*.{rs,toml,json}',
    '../**/*.{ts,tsx,css}',
    '!../**/*.test.{ts,tsx}',
    '../../*.{json,ts}',
    '../../../../*.{md,toml}',
    '../../../../config/*.toml',
    '../../../../scripts/*.sh',
    '../../../../docs/**/*.json',
  ],
  { query: '?raw', import: 'default' },
);

function repositoryPath(key: string): string {
  if (key.startsWith('../../../../')) return key.slice(12);
  if (key.startsWith('../../../desktop/')) return `apps/desktop/${key.slice(17)}`;
  if (key.startsWith('../../')) return `apps/web/${key.slice(6)}`;
  return `apps/web/src/${key.slice(3)}`;
}

export const sourceFiles = new Map(
  Object.entries(rawFiles).map(([key, read]) => [repositoryPath(key), read]),
);
const imageFiles = import.meta.glob<string>('../../../../docs/**/*.{png,jpg,svg}', {
  query: '?url',
  import: 'default',
  eager: true,
});
const images = new Map(Object.entries(imageFiles).map(([key, url]) => [repositoryPath(key), url]));

export function resolveRepositoryPath(
  from: string,
  href: string,
): { path: string; section: string } {
  const url = new URL(href, `https://lavender.invalid/${from}`);
  return {
    path: decodeURIComponent(url.pathname.slice(1)),
    section: decodeURIComponent(url.hash.slice(1)),
  };
}

export function resolveLink(doc: Doc, href: string): { href: string; source?: string } {
  if (/^(https?:|mailto:)/i.test(href)) return { href };
  if (href.startsWith('#/')) return { href };
  if (/^[a-z][a-z\d+.-]*:/i.test(href) || href.startsWith('//')) return { href: '' };
  const { path, section } = resolveRepositoryPath(doc.path, href);
  const target =
    docByPath.get(path) ??
    (path.endsWith('/')
      ? (documents.find((entry) => entry.path === `${path}README.md`) ??
        documents.find((entry) => entry.path.startsWith(path)))
      : undefined);
  if (target) return { href: docHref(target.id, section) };
  if (sourceFiles.has(path)) return { href: `#source-${encodeURIComponent(path)}`, source: path };
  return {
    href: `https://github.com/alexcircuits/lavender/${path.endsWith('/') ? 'tree' : 'blob'}/main/${path}${section ? `#${section}` : ''}`,
  };
}

export function resolveImage(doc: Doc, href: string): string | undefined {
  if (/^(https?:|\/\/|data:)/i.test(href)) return undefined;
  return images.get(resolveRepositoryPath(doc.path, href).path);
}

export interface SearchResult {
  doc: Doc;
  heading?: Heading;
  snippet: string;
  score: number;
}

export function searchDocs(query: string): SearchResult[] {
  const words = query.toLowerCase().trim().split(/\s+/).filter(Boolean);
  if (!words.length)
    return mainGuides.slice(1, 7).map((doc) => ({ doc, snippet: doc.group, score: 0 }));
  return documents
    .flatMap((doc): SearchResult[] => {
      const title = doc.title.toLowerCase();
      const text = doc.searchable.toLowerCase();
      if (!words.every((word) => `${title} ${text}`.includes(word))) return [];
      const heading = doc.headings.find((entry) =>
        words.every((word) => entry.text.toLowerCase().includes(word)),
      );
      const position = Math.max(0, text.indexOf(words[0] ?? '') - 55);
      const snippet = `${position ? '…' : ''}${doc.searchable.slice(position, position + 160)}${text.length > position + 160 ? '…' : ''}`;
      return [
        {
          doc,
          ...(heading ? { heading } : {}),
          snippet,
          score:
            words.reduce((score, word) => score + (title.includes(word) ? 12 : 1), 0) +
            (heading ? 5 : 0) +
            (mainGuides.some((guide) => guide.id === doc.id) ? 6 : 0),
        },
      ];
    })
    .sort((a, b) => b.score - a.score)
    .slice(0, 18);
}
