import { ArrowLeftIcon } from '@phosphor-icons/react/dist/csr/ArrowLeft';
import { ArrowRightIcon } from '@phosphor-icons/react/dist/csr/ArrowRight';
import { ArrowSquareOutIcon } from '@phosphor-icons/react/dist/csr/ArrowSquareOut';
import { BookOpenIcon } from '@phosphor-icons/react/dist/csr/BookOpen';
import { GithubLogoIcon } from '@phosphor-icons/react/dist/csr/GithubLogo';
import { ListIcon } from '@phosphor-icons/react/dist/csr/List';
import { CommandEmpty, CommandInput, CommandItem, CommandList, CommandRoot } from 'cmdk';
import { lazy, Suspense, useCallback, useEffect, useRef, useState } from 'react';
import { Button, Dialog, IconButton, Kbd, MagnifyingGlassIcon } from '../design';
import { CodeBlock } from './CodeBlock';
import {
  advancedGuides,
  type Doc,
  docById,
  docHref,
  type Heading,
  loadDoc,
  loadSearchIndex,
  mainGuides,
  parseRoute,
  searchDocs,
  sourceFiles,
  sourceLanguage,
} from './content';
import styles from './docs.module.css';
import { Home } from './Home';

const Markdown = lazy(() => import('./Markdown').then((module) => ({ default: module.Markdown })));

function Navigation({ current, onNavigate }: { current: string; onNavigate?: () => void }) {
  function groups(docs: Doc[]) {
    return Array.from(new Set(docs.map((doc) => doc.group))).map((group) => (
      <div key={group} className={styles.navGroup}>
        <p>{group}</p>
        {/* biome-ignore lint/a11y/noRedundantRoles: preserve list semantics in Safari when list-style is none */}
        <ul role="list">
          {docs
            .filter((doc) => doc.group === group)
            .map((doc) => (
              <li key={doc.id}>
                <a
                  href={docHref(doc.id)}
                  aria-current={current === doc.id ? 'page' : undefined}
                  onClick={onNavigate}
                >
                  {doc.title}
                </a>
              </li>
            ))}
        </ul>
      </div>
    ));
  }
  return (
    <nav aria-label="Documentation" className={styles.navigation}>
      {groups(mainGuides)}
      <details
        className={styles.advanced}
        key={advancedGuides.some((doc) => doc.id === current) ? 'open' : 'closed'}
        open={advancedGuides.some((doc) => doc.id === current)}
      >
        <summary>Specifications & history</summary>
        <p className={styles.archiveNote}>
          Detailed requirements and historical decisions. Proposed APIs remain targets.
        </p>
        {groups(advancedGuides)}
      </details>
    </nav>
  );
}

function Search({ onDone }: { onDone: () => void }) {
  const [query, setQuery] = useState('');
  const [indexState, setIndexState] = useState<'loading' | 'ready' | 'partial'>('loading');
  useEffect(() => {
    let alive = true;
    loadSearchIndex().then((complete) => {
      if (alive) setIndexState(complete ? 'ready' : 'partial');
    });
    return () => {
      alive = false;
    };
  }, []);
  const results = searchDocs(query);
  return (
    <CommandRoot label="Search documentation" shouldFilter={false} loop vimBindings={false}>
      <div className={styles.searchField}>
        <MagnifyingGlassIcon size={18} aria-hidden />
        <CommandInput
          value={query}
          onValueChange={setQuery}
          className={styles.searchInput}
          placeholder="Search guides, commands, APIs…"
          aria-label="Search documentation"
        />
      </div>
      <CommandList className={styles.searchList}>
        {indexState === 'loading' ? (
          <p className={styles.searchHint} role="status">
            Loading full-text search…
          </p>
        ) : indexState === 'partial' ? (
          <p className={styles.searchHint} role="status">
            Some guides could not load. Available pages are searchable.
          </p>
        ) : null}
        {!query ? (
          <p className={styles.searchHint}>Start with a guide</p>
        ) : (
          <p className={styles.searchHint}>
            {results.length} {results.length === 1 ? 'result' : 'results'}
          </p>
        )}
        {indexState !== 'loading' ? (
          <CommandEmpty className={styles.searchEmpty}>
            No matching guide. Try “provider”, “fixtures” or “WebSocket”.
          </CommandEmpty>
        ) : null}
        {results.map(({ doc, heading, snippet }) => (
          <CommandItem
            className={styles.searchResult}
            key={doc.id}
            value={doc.id}
            onSelect={() => {
              window.location.hash = docHref(doc.id, heading?.id);
              onDone();
            }}
          >
            <BookOpenIcon size={18} aria-hidden />
            <span>
              <strong>
                {doc.title}
                {heading ? <span> / {heading.text}</span> : null}
              </strong>
              <span>{snippet}</span>
            </span>
            <ArrowRightIcon size={16} aria-hidden />
          </CommandItem>
        ))}
      </CommandList>
      <footer className={styles.searchLegend} aria-hidden>
        <span>
          <Kbd>↑</Kbd>
          <Kbd>↓</Kbd> navigate
        </span>
        <span>
          <Kbd>Enter</Kbd> open
        </span>
        <span>
          <Kbd>Esc</Kbd> close
        </span>
      </footer>
    </CommandRoot>
  );
}

const homeHeadings: Heading[] = [
  { id: 'how-it-fits', text: 'How it fits together', level: 2 },
  { id: 'quickstart', text: 'Run locally', level: 2 },
  { id: 'first-change', text: 'Your first contribution', level: 2 },
  { id: 'keep-close', text: 'Useful references', level: 2 },
];

function Outline({ doc, headings }: { doc: Doc; headings: Heading[] }) {
  const [active, setActive] = useState('');
  useEffect(() => {
    let pending = 0;
    function update() {
      const visible = headings.filter(
        (heading) =>
          (document.getElementById(heading.id)?.getBoundingClientRect().top ?? Infinity) <= 140,
      );
      setActive(visible.at(-1)?.id ?? headings[0]?.id ?? '');
      pending = 0;
    }
    function scroll() {
      if (!pending) pending = requestAnimationFrame(update);
    }
    update();
    window.addEventListener('scroll', scroll, { passive: true });
    return () => {
      window.removeEventListener('scroll', scroll);
      cancelAnimationFrame(pending);
    };
  }, [headings]);
  return (
    <aside className={styles.outline} aria-label="On this page">
      <p>On this page</p>
      <nav>
        {/* biome-ignore lint/a11y/noRedundantRoles: preserve list semantics in Safari when list-style is none */}
        <ul role="list">
          {headings.map((heading) => (
            <li key={heading.id} data-level={heading.level}>
              <a
                href={docHref(doc.id, heading.id)}
                aria-current={heading.id === active ? 'location' : undefined}
              >
                {heading.text}
              </a>
            </li>
          ))}
        </ul>
      </nav>
      <div className={styles.outlineHelp}>
        <span>Working on a change?</span>
        <a href={docHref('contributing')}>
          Contribution workflow <ArrowRightIcon size={14} aria-hidden />
        </a>
      </div>
    </aside>
  );
}

function SourceView({ path, markdown }: { path: string; markdown?: string }) {
  const [state, setState] = useState<{ code?: string; error?: boolean }>({});
  useEffect(() => {
    let alive = true;
    setState({});
    if (markdown !== undefined) {
      setState({ code: markdown });
      return;
    }
    const read = sourceFiles.get(path);
    if (!read) {
      setState({ error: true });
      return;
    }
    read()
      .then((code) => {
        if (alive) setState({ code });
      })
      .catch(() => {
        if (alive) setState({ error: true });
      });
    return () => {
      alive = false;
    };
  }, [path, markdown]);
  if (state.error)
    return <p role="alert">This source file could not load. Close this view and try again.</p>;
  if (state.code === undefined) return <p role="status">Loading source…</p>;
  return <CodeBlock code={state.code} language={sourceLanguage(path)} label={path} />;
}

export function DocsApp() {
  const [route, setRoute] = useState(() => parseRoute(window.location.hash));
  const [searchOpen, setSearchOpen] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [source, setSource] = useState<{ path: string; markdown?: string } | null>(null);
  const previousId = useRef(route.id);
  const [loadedDoc, setLoadedDoc] = useState<Doc | null>(null);
  const [loadError, setLoadError] = useState('');
  const doc = loadedDoc?.id === route.id ? loadedDoc : docById.get(route.id);
  const contentReady = doc?.loaded ?? false;
  const onReady = useCallback(() => {
    if (route.section) document.getElementById(route.section)?.scrollIntoView({ block: 'start' });
  }, [route.section]);
  useEffect(() => {
    let alive = true;
    setLoadError('');
    if (docById.has(route.id))
      loadDoc(route.id)
        .then((result) => {
          if (alive) setLoadedDoc(result);
        })
        .catch(() => {
          if (alive) setLoadError(route.id);
        });
    return () => {
      alive = false;
    };
  }, [route.id]);
  useEffect(() => {
    const onHash = () => setRoute(parseRoute(window.location.hash));
    window.addEventListener('hashchange', onHash);
    const onKey = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        setSearchOpen((open) => !open);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('hashchange', onHash);
      window.removeEventListener('keydown', onKey);
    };
  }, []);
  useEffect(() => {
    document.title = `${doc?.title ?? 'Page not found'} · Lavender docs`;
    const frame = requestAnimationFrame(() => {
      if (route.section && contentReady)
        document.getElementById(route.section)?.scrollIntoView({ block: 'start' });
      else window.scrollTo({ top: 0 });
      if (previousId.current !== route.id)
        document.getElementById('page-title')?.focus({ preventScroll: true });
      previousId.current = route.id;
    });
    return () => cancelAnimationFrame(frame);
  }, [route, doc, contentReady]);
  const index = mainGuides.findIndex((item) => item.id === doc?.id);
  const previous = mainGuides[index - 1];
  const next = index >= 0 ? mainGuides[index + 1] : undefined;
  return (
    <div className={styles.root}>
      <a
        className={styles.skipLink}
        href={docHref(route.id, 'reading')}
        onClick={() => document.getElementById('reading')?.focus()}
      >
        Skip to content
      </a>
      <header className={styles.header}>
        <div className={styles.brandGroup}>
          <a
            className={styles.brand}
            href={docHref('overview')}
            aria-label="Lavender documentation home"
          >
            <img src="/favicon.svg" width={28} height={28} alt="" />
            <span>Lavender</span>
          </a>
          <span className={styles.brandDivider} />
          <span className={styles.docsLabel}>Developer docs</span>
        </div>
        <div className={styles.headerActions}>
          <Dialog
            open={searchOpen}
            onOpenChange={setSearchOpen}
            title="Search documentation"
            className={styles.searchDialog}
            trigger={
              <button
                type="button"
                className={styles.searchTrigger}
                aria-label="Search documentation"
              >
                <MagnifyingGlassIcon size={16} aria-hidden />
                <span>Search documentation</span>
                <Kbd>⌘ K</Kbd>
              </button>
            }
          >
            {searchOpen ? <Search onDone={() => setSearchOpen(false)} /> : null}
          </Dialog>
          <a
            href="https://github.com/alexcircuits/lavender"
            target="_blank"
            rel="noreferrer"
            className={styles.repoLink}
            aria-label="Lavender on GitHub"
          >
            <GithubLogoIcon size={20} aria-hidden />
          </a>
          <a className={styles.workspaceLink} href="/">
            Open workspace <ArrowSquareOutIcon size={14} aria-hidden />
          </a>
          <Dialog
            title="Documentation navigation"
            open={menuOpen}
            onOpenChange={setMenuOpen}
            className={styles.menuDialog}
            trigger={
              <IconButton
                icon={ListIcon}
                label="Open documentation navigation"
                className={styles.mobileMenu}
              />
            }
          >
            <Navigation current={route.id} onNavigate={() => setMenuOpen(false)} />
          </Dialog>
        </div>
      </header>
      <div className={styles.layout}>
        <div className={styles.sidebar}>
          <Navigation current={route.id} />
          <div className={styles.sidebarFooter}>
            <span>Built from the repository guides.</span>
            <a href={docHref('verification')}>
              Implementation & verification <ArrowRightIcon size={12} aria-hidden />
            </a>
          </div>
        </div>
        <main id="reading" className={styles.reading} tabIndex={-1}>
          <article
            key={route.id}
            className={doc?.id === 'overview' ? undefined : styles.guideArticle}
          >
            {doc ? (
              <>
                {doc.id === 'overview' ? (
                  <Home />
                ) : (
                  <>
                    <div className={styles.articleHeader}>
                      <p className={styles.eyebrow}>{doc.group}</p>
                      <h1 id="page-title" tabIndex={-1}>
                        {doc.title}
                      </h1>
                      <div className={styles.articleMeta}>
                        <span>
                          {Math.max(1, Math.ceil(doc.searchable.split(' ').length / 220))} min read
                        </span>
                        <span className={styles.metaDot}>·</span>
                        <span>Repository guide</span>
                        <Button
                          size="sm"
                          variant="ghost"
                          disabled={!doc.loaded}
                          onClick={() => setSource({ path: doc.path, markdown: doc.markdown })}
                        >
                          View Markdown
                        </Button>
                      </div>
                    </div>
                    {loadError === route.id ? (
                      <p role="alert">
                        This guide could not load.{' '}
                        <Button onClick={() => window.location.reload()}>Reload page</Button>
                      </p>
                    ) : doc.loaded ? (
                      <Suspense
                        fallback={
                          <p role="status" className={styles.loading}>
                            Preparing guide…
                          </p>
                        }
                      >
                        <Markdown
                          doc={doc}
                          onSource={(path) => setSource({ path })}
                          onReady={onReady}
                        />
                      </Suspense>
                    ) : (
                      <p className={styles.loading} role="status">
                        Loading guide…
                      </p>
                    )}
                  </>
                )}
                <nav className={styles.pageNavigation} aria-label="Next and previous guides">
                  {previous ? (
                    <a href={docHref(previous.id)}>
                      <span>
                        <ArrowLeftIcon size={14} aria-hidden /> Previous
                      </span>
                      <strong>{previous.title}</strong>
                    </a>
                  ) : (
                    <span />
                  )}
                  {next ? (
                    <a href={docHref(next.id)} className={styles.nextPage}>
                      <span>
                        Next <ArrowRightIcon size={14} aria-hidden />
                      </span>
                      <strong>{next.title}</strong>
                    </a>
                  ) : null}
                </nav>
                <footer className={styles.articleFooter}>
                  <span>Lavender · NoirSystems</span>
                  <Button
                    size="sm"
                    variant="ghost"
                    disabled={!doc.loaded}
                    onClick={() => setSource({ path: doc.path, markdown: doc.markdown })}
                  >
                    Read page source
                  </Button>
                </footer>
              </>
            ) : (
              <div className={styles.notFound}>
                <p className={styles.eyebrow}>Documentation</p>
                <h1 id="page-title" tabIndex={-1}>
                  This guide could not be found.
                </h1>
                <p>Use the guide navigation or search to find the page you need.</p>
                <a href={docHref('overview')} className={styles.primaryLink}>
                  Back to overview <ArrowRightIcon size={16} aria-hidden />
                </a>
              </div>
            )}
          </article>
        </main>
        {doc ? (
          <Outline doc={doc} headings={doc.id === 'overview' ? homeHeadings : doc.headings} />
        ) : null}
      </div>
      <Dialog
        open={source !== null}
        onOpenChange={(open) => {
          if (!open) setSource(null);
        }}
        title="Repository source"
        description="Read the file from this documentation build."
        className={styles.sourceDialog}
      >
        {source ? <SourceView key={source.path} {...source} /> : null}
      </Dialog>
    </div>
  );
}
