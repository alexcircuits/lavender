import { Children, isValidElement, lazy, type ReactNode, Suspense, useEffect } from 'react';
import ReactMarkdown, { defaultUrlTransform } from 'react-markdown';
import rehypeSlug from 'rehype-slug';
import remarkGfm from 'remark-gfm';
import { CodeBlock } from './CodeBlock';
import { type Doc, docHref, resolveImage, resolveLink } from './content';
import styles from './docs.module.css';

const Diagram = lazy(() => import('./Diagram'));
function codeText(children: ReactNode): string {
  return Children.toArray(children)
    .map((child) => {
      if (typeof child === 'string' || typeof child === 'number') return String(child);
      if (isValidElement<{ children?: ReactNode }>(child)) return codeText(child.props.children);
      return '';
    })
    .join('');
}

export function Markdown({
  doc,
  onSource,
  onReady,
}: {
  doc: Doc;
  onSource: (path: string) => void;
  onReady: () => void;
}) {
  useEffect(() => {
    onReady();
  }, [onReady]);
  return (
    <div className={styles.prose}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeSlug]}
        skipHtml
        urlTransform={defaultUrlTransform}
        components={{
          a: ({ href = '', children }) => {
            const target = resolveLink(doc, href);
            return (
              <a
                href={target.href}
                {...(target.href.startsWith('http') ? { target: '_blank', rel: 'noreferrer' } : {})}
                onClick={
                  target.source
                    ? (event) => {
                        event.preventDefault();
                        if (target.source) onSource(target.source);
                      }
                    : undefined
                }
              >
                {children}
              </a>
            );
          },
          img: ({ src = '', alt = '' }) => {
            const url = resolveImage(doc, src);
            return url ? <img src={url} alt={alt} loading="lazy" /> : <span>{alt}</span>;
          },
          h2: ({ id, children }) => (
            <h2 id={id}>
              <a className={styles.headingLink} href={docHref(doc.id, id)}>
                {children}
                <span aria-hidden>#</span>
              </a>
            </h2>
          ),
          h3: ({ id, children }) => (
            <h3 id={id}>
              <a className={styles.headingLink} href={docHref(doc.id, id)}>
                {children}
                <span aria-hidden>#</span>
              </a>
            </h3>
          ),
          table: ({ children }) => (
            <section
              className={styles.tableScroll}
              // biome-ignore lint/a11y/noNoninteractiveTabindex: keyboard users must be able to scroll wide reference tables
              tabIndex={0}
              aria-label="Scrollable reference table"
            >
              <table>{children}</table>
            </section>
          ),
          pre: ({ children }) => {
            const child = Children.toArray(children)[0];
            const language = isValidElement<{ className?: string }>(child)
              ? (child.props.className?.replace('language-', '') ?? 'text')
              : 'text';
            const code = codeText(children);
            return language === 'mermaid' ? (
              <Suspense
                fallback={
                  <figure className={styles.diagram}>
                    <div className={styles.codeHeader}>Architecture diagram</div>
                    <div className={styles.diagramCanvas}>
                      <p role="status">Loading diagram…</p>
                    </div>
                    <figcaption>
                      Follow the shared core through either platform adapter. Scroll the diagram to
                      explore.
                    </figcaption>
                  </figure>
                }
              >
                <Diagram code={code} />
              </Suspense>
            ) : (
              <CodeBlock code={code} language={language} />
            );
          },
        }}
      >
        {doc.body}
      </ReactMarkdown>
    </div>
  );
}
