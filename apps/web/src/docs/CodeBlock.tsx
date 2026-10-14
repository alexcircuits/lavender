import { Highlight, type Language, type PrismTheme } from 'prism-react-renderer';
import { useEffect, useState } from 'react';
import { Button, CheckIcon, CopyIcon } from '../design';
import styles from './docs.module.css';

const theme: PrismTheme = {
  plain: { color: 'var(--ink)', backgroundColor: 'var(--well)' },
  styles: [
    { types: ['comment', 'prolog', 'doctype', 'cdata'], style: { color: 'var(--ink-3)' } },
    { types: ['keyword', 'operator', 'tag', 'atrule'], style: { color: 'var(--accent-ink)' } },
    { types: ['string', 'char', 'attr-value', 'inserted'], style: { color: 'var(--map-space)' } },
    { types: ['function', 'class-name', 'number', 'boolean'], style: { color: 'var(--ink)' } },
    { types: ['punctuation'], style: { color: 'var(--ink-2)' } },
  ],
};

export function CodeBlock({
  code,
  language = 'text',
  label,
}: {
  code: string;
  language?: string;
  label?: string;
}) {
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState(false);
  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setCopied(false), 2000);
    return () => window.clearTimeout(timer);
  }, [copied]);
  const normalized =
    ({ sh: 'bash', shell: 'bash', ts: 'typescript', rs: 'rust' } as Record<string, string>)[
      language
    ] ?? language;
  return (
    <div className={styles.codeFrame}>
      <div className={styles.codeHeader}>
        <span>{label ?? language}</span>
        <Button
          variant="ghost"
          size="sm"
          icon={copied ? CheckIcon : CopyIcon}
          aria-label={`Copy ${label ?? language} code`}
          onClick={async () => {
            setError(false);
            setCopied(false);
            try {
              await navigator.clipboard.writeText(code);
              setCopied(true);
            } catch {
              setError(true);
            }
          }}
        >
          {copied ? 'Copied' : 'Copy'}
        </Button>
      </div>
      <Highlight code={code.replace(/\n$/, '')} language={normalized as Language} theme={theme}>
        {({ tokens, getLineProps, getTokenProps }) => (
          <section
            // biome-ignore lint/a11y/noNoninteractiveTabindex: keyboard users must be able to scroll long code
            tabIndex={0}
            aria-label={`${label ?? language} code`}
            className={styles.codeScroll}
          >
            <pre>
              <code>
                {tokens.map((line, i) => (
                  // A token's position is its identity within this immutable code example.
                  // biome-ignore lint/suspicious/noArrayIndexKey: immutable token positions
                  <span key={i} {...getLineProps({ line })}>
                    {line.map((token, j) => (
                      // biome-ignore lint/suspicious/noArrayIndexKey: immutable token positions
                      <span key={j} {...getTokenProps({ token })}>
                        {token.empty ? '' : token.content}
                      </span>
                    ))}
                    {i < tokens.length - 1 ? '\n' : ''}
                  </span>
                ))}
              </code>
            </pre>
          </section>
        )}
      </Highlight>
      <span role="status" className={error ? styles.copyError : 'visually-hidden'}>
        {error
          ? 'Copy unavailable. Select the code to copy it.'
          : copied
            ? 'Code copied to clipboard.'
            : ''}
      </span>
    </div>
  );
}
