import { createStore } from 'zustand/vanilla';

export type ToastKind = 'info' | 'success' | 'error';

export interface ToastAction {
  label: string;
  onAction: () => void;
}

export interface ToastInput {
  kind?: ToastKind;
  /** Uses the verb of the action that caused it ("Link copied"). */
  title: string;
  description?: string;
  /** Errors persist and should carry an action (Retry, Configure). */
  action?: ToastAction;
}

export interface Toast extends Required<Pick<ToastInput, 'kind' | 'title'>> {
  id: number;
  description?: string;
  action?: ToastAction;
}

export interface ToastState {
  /** All pending toasts, oldest first; at most MAX_VISIBLE_TOASTS are shown at once. */
  toasts: ReadonlyArray<Toast>;
}

/** Spec (§7): at most 2 visible; info and success auto-dismiss after 4 s; errors persist. */
export const MAX_VISIBLE_TOASTS = 2;
export const TOAST_DURATION_MS = 4000;

let nextId = 1;
const timers = new Map<number, ReturnType<typeof setTimeout>>();

export const toastStore = createStore<ToastState>(() => ({ toasts: [] }));

function visible(toasts: ReadonlyArray<Toast>): ReadonlyArray<Toast> {
  return toasts.slice(0, MAX_VISIBLE_TOASTS);
}

/** Starts the auto-dismiss timer for toasts that just became visible. */
function scheduleVisible(): void {
  for (const toast of visible(toastStore.getState().toasts)) {
    if (toast.kind === 'error' || timers.has(toast.id)) continue;
    timers.set(
      toast.id,
      setTimeout(() => dismissToast(toast.id), TOAST_DURATION_MS),
    );
  }
}

export function showToast(input: ToastInput): number {
  const id = nextId++;
  const toast: Toast = {
    id,
    kind: input.kind ?? 'info',
    title: input.title,
    ...(input.description === undefined ? {} : { description: input.description }),
    ...(input.action === undefined ? {} : { action: input.action }),
  };
  toastStore.setState((state) => ({ toasts: [...state.toasts, toast] }));
  scheduleVisible();
  return id;
}

export function dismissToast(id: number): void {
  const timer = timers.get(id);
  if (timer !== undefined) clearTimeout(timer);
  timers.delete(id);
  toastStore.setState((state) => ({ toasts: state.toasts.filter((toast) => toast.id !== id) }));
  scheduleVisible();
}

export function clearToasts(): void {
  for (const timer of timers.values()) clearTimeout(timer);
  timers.clear();
  toastStore.setState({ toasts: [] });
}

export function visibleToasts(state: ToastState): ReadonlyArray<Toast> {
  return visible(state.toasts);
}
