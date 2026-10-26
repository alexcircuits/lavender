import { useEffect, useRef, useState } from 'react';
import styles from './SplitFlap.module.css';

export interface SplitFlapProps {
  readonly text: string;
  readonly className?: string | undefined;
  readonly minColumns?: number | undefined;
  readonly staggerMs?: number | undefined;
}

/**
 * Mechanical Solari departure-board character flip component (NoirSystems flat aesthetic).
 * Flips characters sequentially left-to-right when text mutations occur.
 */
export function SplitFlap({ text, className, minColumns = 0, staggerMs = 25 }: SplitFlapProps) {
  const normalized = text.toUpperCase();
  const targetLen = Math.max(minColumns, normalized.length);
  const padded = normalized.padEnd(targetLen, ' ');

  const [displayed, setDisplayed] = useState<string>(padded);
  const [flippingIndices, setFlippingIndices] = useState<ReadonlySet<number>>(new Set());
  const prevPaddedRef = useRef<string>(padded);

  useEffect(() => {
    if (padded === prevPaddedRef.current) return;
    const prev = prevPaddedRef.current;
    prevPaddedRef.current = padded;

    const changed = new Set<number>();
    const maxLen = Math.max(prev.length, padded.length);
    for (let i = 0; i < maxLen; i++) {
      if (prev[i] !== padded[i]) {
        changed.add(i);
      }
    }

    if (changed.size === 0) return;

    setFlippingIndices(changed);

    // Stagger character settles
    const timers: NodeJS.Timeout[] = [];
    changed.forEach((index) => {
      const delay = index * staggerMs;
      const t = setTimeout(() => {
        setDisplayed((curr) => {
          const arr = [...curr.padEnd(targetLen, ' ')];
          arr[index] = padded[index] ?? ' ';
          return arr.join('');
        });
      }, delay);
      timers.push(t);
    });

    const settleTimer = setTimeout(
      () => {
        setDisplayed(padded);
        setFlippingIndices(new Set());
      },
      changed.size * staggerMs + 240,
    );
    timers.push(settleTimer);

    return () => {
      for (const t of timers) clearTimeout(t);
    };
  }, [padded, targetLen, staggerMs]);

  return (
    <div role="status" aria-label={text} className={`${styles.board} ${className ?? ''}`}>
      {[...displayed].map((char, idx) => {
        const isFlipping = flippingIndices.has(idx);
        return (
          <span
            // biome-ignore lint/suspicious/noArrayIndexKey: Fixed board column indexes
            key={idx}
            className={`${styles.cell} ${isFlipping ? styles.flipping : ''}`}
            aria-hidden="true"
          >
            <span className={styles.glyph}>{char === ' ' ? '\u00A0' : char}</span>
          </span>
        );
      })}
    </div>
  );
}
