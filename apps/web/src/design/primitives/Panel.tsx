import {
  type ComponentPropsWithRef,
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
  useId,
  useRef,
} from 'react';
import { cx } from '../cx';
import styles from './Panel.module.css';

export interface PanelResize {
  width: number;
  min: number;
  max: number;
  onWidthChange: (width: number) => void;
  /** Which edge carries the handle: `end` for a left-docked panel, `start` for a right one. */
  edge?: 'start' | 'end';
  /** Accessible name of the handle, e.g. "Resize layers panel". */
  label: string;
}

export interface PanelProps extends Omit<ComponentPropsWithRef<'section'>, 'title'> {
  title: string;
  /** Actions in the 40 px header: IconButtons, a menu trigger. */
  actions?: ReactNode;
  /** Optional line under the header (counts, filters) that does not scroll. */
  toolbar?: ReactNode;
  footer?: ReactNode;
  /** 12 px padding instead of 16. */
  dense?: boolean;
  /** Heading level for the title; panels in the shell are h2. */
  headingLevel?: 2 | 3;
  resize?: PanelResize;
  children?: ReactNode;
}

const KEY_STEP = 16;
const KEY_STEP_LARGE = 64;

/**
 * An island: opaque `--island` panel over the globe, `--r-lg` squircle, 40 px header with a
 * title and actions, a scrolling body, and an optional resizable edge (4 px hit area, hairline
 * on hover, arrow keys when focused). Islands never nest.
 */
export function Panel({
  title,
  actions,
  toolbar,
  footer,
  dense = false,
  headingLevel = 2,
  resize,
  className,
  children,
  ...rest
}: PanelProps) {
  const titleId = useId();
  const Heading = headingLevel === 2 ? 'h2' : 'h3';
  return (
    <section
      {...rest}
      aria-labelledby={titleId}
      data-dense={dense ? '' : undefined}
      className={cx(styles.panel, className)}
    >
      <header className={styles.header}>
        <Heading id={titleId} className={styles.title}>
          {title}
        </Heading>
        {actions ? <div className={styles.actions}>{actions}</div> : null}
      </header>
      {toolbar ? <div className={styles.toolbar}>{toolbar}</div> : null}
      <div className={styles.body}>{children}</div>
      {footer ? <footer className={styles.footer}>{footer}</footer> : null}
      {resize ? <ResizeHandle {...resize} /> : null}
    </section>
  );
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function ResizeHandle({ width, min, max, onWidthChange, edge = 'end', label }: PanelResize) {
  const drag = useRef<{ startX: number; startWidth: number } | null>(null);
  const direction = edge === 'end' ? 1 : -1;

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = { startX: event.clientX, startWidth: width };
  };
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (!drag.current) return;
    const delta = (event.clientX - drag.current.startX) * direction;
    onWidthChange(clamp(Math.round(drag.current.startWidth + delta), min, max));
  };
  const onPointerUp = (event: PointerEvent<HTMLDivElement>) => {
    drag.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const step = event.shiftKey ? KEY_STEP_LARGE : KEY_STEP;
    const keys: Record<string, number> = {
      ArrowRight: width + step * direction,
      ArrowLeft: width - step * direction,
      Home: min,
      End: max,
    };
    const next = keys[event.key];
    if (next === undefined) return;
    event.preventDefault();
    onWidthChange(clamp(next, min, max));
  };

  return (
    // biome-ignore lint/a11y/useSemanticElements: a focusable splitter is role="separator" by spec; <hr> cannot take focus or values.
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuenow={width}
      aria-valuemin={min}
      aria-valuemax={max}
      tabIndex={0}
      data-edge={edge}
      className={styles.resize}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
      onKeyDown={onKeyDown}
    />
  );
}
