import type { CSSProperties } from 'react';
import { cx } from '../cx';
import styles from './Skeleton.module.css';

export interface SkeletonProps {
  /** CSS width; match the real layout's geometry. */
  width?: CSSProperties['width'];
  /** CSS height; defaults to one line of body text. */
  height?: CSSProperties['height'];
  shape?: 'line' | 'block' | 'pill';
  className?: string | undefined;
}

/** Static `--island-hover` placeholder at the real layout's geometry. No shimmer, nothing loops. */
export function Skeleton({ width = '100%', height, shape = 'line', className }: SkeletonProps) {
  return (
    <span
      aria-hidden="true"
      data-shape={shape}
      className={cx(styles.skeleton, className)}
      style={{ width, ...(height === undefined ? {} : { height }) }}
    />
  );
}
