import { useEffect, useId, useRef, useState } from 'react';
import { Button } from '../design';
import { CodeBlock } from './CodeBlock';
import styles from './docs.module.css';

/** Mermaid runs only for a diagram. Strict mode sanitizes checked-in diagram definitions. */
export default function Diagram({ code }: { code: string }) {
  const id = `diagram-${useId().replace(/[^a-z\d]/gi, '')}`;
  const target = useRef<HTMLDivElement>(null);
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');
  const [showSource, setShowSource] = useState(false);
  useEffect(() => {
    let alive = true;
    setStatus('loading');
    import('mermaid')
      .then(async ({ default: mermaid }) => {
        const tokens = getComputedStyle(document.documentElement);
        const color = (name: string) => tokens.getPropertyValue(name).trim();
        mermaid.initialize({
          startOnLoad: false,
          securityLevel: 'strict',
          theme: 'base',
          fontFamily: 'Geist Variable, sans-serif',
          flowchart: { htmlLabels: false, useMaxWidth: false },
          themeVariables: {
            darkMode: true,
            primaryColor: color('--island'),
            primaryTextColor: color('--ink'),
            primaryBorderColor: color('--hairline-strong'),
            lineColor: color('--ink-3'),
            secondaryColor: color('--accent-subtle'),
            tertiaryColor: color('--well'),
            edgeLabelBackground: color('--well'),
          },
        });
        const { svg } = await mermaid.render(id, code);
        if (!alive || !target.current) return;
        const documentSvg = new DOMParser().parseFromString(svg, 'image/svg+xml').documentElement;
        documentSvg.setAttribute('role', 'img');
        documentSvg.setAttribute('aria-label', 'Lavender architecture diagram');
        target.current.replaceChildren(document.importNode(documentSvg, true));
        setStatus('ready');
      })
      .catch(() => {
        if (alive) setStatus('error');
      });
    return () => {
      alive = false;
    };
  }, [code, id]);
  return (
    <figure className={styles.diagram}>
      <div className={styles.codeHeader}>
        <span>Architecture diagram</span>
        <Button size="sm" variant="ghost" onClick={() => setShowSource(!showSource)}>
          {showSource ? 'Hide definition' : 'View definition'}
        </Button>
      </div>
      {status === 'loading' ? (
        <p role="status" className={styles.diagramStatus}>
          Drawing diagram…
        </p>
      ) : null}
      {status === 'error' ? (
        <p role="status" className={styles.diagramStatus}>
          Diagram could not render. Its definition is available below.
        </p>
      ) : null}
      <section
        ref={target}
        className={styles.diagramCanvas}
        // biome-ignore lint/a11y/noNoninteractiveTabindex: keyboard users must be able to scroll the two-dimensional diagram
        tabIndex={0}
        aria-label="Scrollable architecture diagram"
      />
      {showSource || status === 'error' ? <CodeBlock code={code} language="mermaid" /> : null}
      <figcaption>
        Follow the shared core through either platform adapter. Scroll the diagram to explore.
      </figcaption>
    </figure>
  );
}
