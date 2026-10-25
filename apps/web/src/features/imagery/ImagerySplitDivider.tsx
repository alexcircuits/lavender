import { useCallback, useRef } from 'react';
import { useUi } from '../../runtime';
import styles from './imagerySplit.module.css';
import { useImageryStore } from './imageryStore';

export function ImagerySplitDivider() {
  const mode = useImageryStore((s) => s.mode);
  const box = useImageryStore((s) => s.box);
  const slotA = useImageryStore((s) => s.slotA);
  const slotB = useImageryStore((s) => s.slotB);
  const splitPosition = useImageryStore((s) => s.splitPosition);
  const setSplitPosition = useImageryStore((s) => s.setSplitPosition);
  const globe = useUi((s) => s.globe);

  const isDragging = useRef(false);

  const updatePosition = useCallback(
    (clientX: number) => {
      const width = window.innerWidth;
      if (width <= 0) return;
      const fraction = Math.max(0.05, Math.min(0.95, clientX / width));
      setSplitPosition(fraction);
      globe?.setRecentImagerySplitPosition(fraction);
    },
    [setSplitPosition, globe],
  );

  const onPointerDown = (e: React.PointerEvent<HTMLButtonElement>) => {
    isDragging.current = true;
    e.currentTarget.setPointerCapture(e.pointerId);
  };

  const onPointerMove = (e: React.PointerEvent<HTMLButtonElement>) => {
    if (!isDragging.current) return;
    updatePosition(e.clientX);
  };

  const onPointerUp = (e: React.PointerEvent<HTMLButtonElement>) => {
    if (isDragging.current) {
      isDragging.current = false;
      try {
        e.currentTarget.releasePointerCapture(e.pointerId);
      } catch {
        // pointer capture already lost
      }
    }
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLButtonElement>) => {
    const step = e.shiftKey ? 0.1 : 0.02;
    if (e.key === 'ArrowLeft') {
      const next = Math.max(0.05, splitPosition - step);
      setSplitPosition(next);
      globe?.setRecentImagerySplitPosition(next);
      e.preventDefault();
    } else if (e.key === 'ArrowRight') {
      const next = Math.min(0.95, splitPosition + step);
      setSplitPosition(next);
      globe?.setRecentImagerySplitPosition(next);
      e.preventDefault();
    } else if (e.key === 'Home') {
      setSplitPosition(0.05);
      globe?.setRecentImagerySplitPosition(0.05);
      e.preventDefault();
    } else if (e.key === 'End') {
      setSplitPosition(0.95);
      globe?.setRecentImagerySplitPosition(0.95);
      e.preventDefault();
    }
  };

  if (mode === 'image' || !box || !slotA) {
    return null;
  }

  const labelA = `${slotA.product} ${slotA.day}`;
  const labelB = mode === 'basemap' ? 'BASEMAP' : slotB ? `${slotB.product} ${slotB.day}` : 'B';

  return (
    <div
      className={styles.dividerContainer}
      style={{ left: `${splitPosition * 100}%` }}
      data-testid="imagery-split-divider"
    >
      <div className={styles.dividerLine} />
      <div className={styles.handleContainer}>
        <div className={styles.badgeLeft}>{labelA}</div>
        <button
          type="button"
          className={styles.handle}
          role="slider"
          aria-label="Satellite imagery comparison split"
          aria-valuenow={Math.round(splitPosition * 100)}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-orientation="horizontal"
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerUp}
          onKeyDown={onKeyDown}
        >
          ↔
        </button>
        <div className={styles.badgeRight}>{labelB}</div>
      </div>
    </div>
  );
}
