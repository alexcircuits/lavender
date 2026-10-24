/**
 * How React reaches the runtime. Components read stores through narrow selectors (a feed
 * update re-renders the one row that shows it) and reach the platform and the globe only
 * through these hooks, never by import (docs/architecture/frontend.md §2–3).
 *
 * Selectors must return stable values: pick primitives or existing objects, or wrap a
 * selector that builds a new object/array in `useShallow` from 'zustand/react/shallow'.
 */
import { createContext, type ReactNode, useContext } from 'react';
import { useStore } from 'zustand';
import type { GlobeController } from '../globe';
import type { LavenderPlatform } from '../platform';
import type {
  FeedsState,
  PreferencesState,
  SystemState,
  UiState,
  WorkspaceState,
} from '../state/stores';
import type { Runtime } from './createRuntime';

const RuntimeContext = createContext<Runtime | null>(null);

export interface RuntimeProviderProps {
  readonly runtime: Runtime;
  readonly children: ReactNode;
}

export function RuntimeProvider({ runtime, children }: RuntimeProviderProps) {
  return <RuntimeContext.Provider value={runtime}>{children}</RuntimeContext.Provider>;
}

export function useRuntime(): Runtime {
  const runtime = useContext(RuntimeContext);
  if (!runtime) throw new Error('useRuntime must be used inside <RuntimeProvider>');
  return runtime;
}

export const usePlatform = (): LavenderPlatform => useRuntime().platform;

export function useWorkspace<T>(selector: (state: WorkspaceState) => T): T {
  return useStore(useRuntime().stores.workspace, selector);
}

export function useFeeds<T>(selector: (state: FeedsState) => T): T {
  return useStore(useRuntime().stores.feeds, selector);
}

export function useSystem<T>(selector: (state: SystemState) => T): T {
  return useStore(useRuntime().stores.system, selector);
}

export function useUi<T>(selector: (state: UiState) => T): T {
  return useStore(useRuntime().stores.ui, selector);
}

export function usePreferences<T>(selector: (state: PreferencesState) => T): T {
  return useStore(useRuntime().stores.preferences, selector);
}

/** The mounted globe, or null while it loads (controls render disabled until then). */
export const useGlobe = (): GlobeController | null => useUi((s) => s.globe);
