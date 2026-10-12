import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  clearToasts,
  dismissToast,
  MAX_VISIBLE_TOASTS,
  showToast,
  TOAST_DURATION_MS,
  toastStore,
  visibleToasts,
} from './toast-store';

describe('toast store', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    clearToasts();
  });
  afterEach(() => {
    clearToasts();
    vi.useRealTimers();
  });

  const titles = () => visibleToasts(toastStore.getState()).map((toast) => toast.title);

  it('shows at most two at once and queues the rest', () => {
    showToast({ title: 'Link copied' });
    showToast({ title: 'Coordinates copied' });
    showToast({ title: 'Layer turned on' });
    expect(MAX_VISIBLE_TOASTS).toBe(2);
    expect(titles()).toEqual(['Link copied', 'Coordinates copied']);
    expect(toastStore.getState().toasts).toHaveLength(3);
  });

  it('auto-dismisses info and success after 4 s, then shows the next one', () => {
    showToast({ title: 'Link copied', kind: 'success' });
    showToast({ title: 'Coordinates copied' });
    showToast({ title: 'Layer turned on' });
    vi.advanceTimersByTime(TOAST_DURATION_MS - 1);
    expect(titles()).toHaveLength(2);
    vi.advanceTimersByTime(1);
    expect(titles()).toEqual(['Layer turned on']);
    vi.advanceTimersByTime(TOAST_DURATION_MS);
    expect(titles()).toEqual([]);
  });

  it('keeps errors until they are dismissed', () => {
    const id = showToast({
      kind: 'error',
      title: 'Earthquakes are unavailable',
      action: { label: 'Retry', onAction: () => undefined },
    });
    vi.advanceTimersByTime(TOAST_DURATION_MS * 10);
    expect(titles()).toEqual(['Earthquakes are unavailable']);
    dismissToast(id);
    expect(titles()).toEqual([]);
  });
});
