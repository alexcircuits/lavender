import { create } from 'zustand';
import type { BBox } from '../../contracts';
import type { ImageryCandidate, RankedLatest } from '../../platform/recentImageryTypes';

export type ImageryMode = 'image' | 'basemap' | 'ab';

export interface RecentImageryState {
  readonly box: BBox | null;
  readonly days: number;
  readonly mode: ImageryMode;
  readonly opacity: number;
  readonly splitPosition: number;
  readonly slotA: ImageryCandidate | null;
  readonly slotB: ImageryCandidate | null;
  readonly candidates: readonly ImageryCandidate[];
  readonly tileTemplates: Record<string, string>;
  readonly rankedLatest: RankedLatest | null;
  readonly attribution: string | null;
  readonly isSearching: boolean;
  readonly isDrawingBox: boolean;
  readonly error: string | null;

  setBox(box: BBox | null): void;
  setDays(days: number): void;
  setMode(mode: ImageryMode): void;
  setOpacity(opacity: number): void;
  setSplitPosition(pos: number): void;
  setSlotA(candidate: ImageryCandidate | null): void;
  setSlotB(candidate: ImageryCandidate | null): void;
  setCandidates(
    candidates: readonly ImageryCandidate[],
    tileTemplates: Record<string, string>,
    rankedLatest: RankedLatest | null,
    attribution: string | null,
  ): void;
  setIsSearching(searching: boolean): void;
  setIsDrawingBox(drawing: boolean): void;
  setError(error: string | null): void;
  clear(): void;
}

export const useImageryStore = create<RecentImageryState>((set) => ({
  box: null,
  days: 14,
  mode: 'image',
  opacity: 1.0,
  splitPosition: 0.5,
  slotA: null,
  slotB: null,
  candidates: [],
  tileTemplates: {},
  rankedLatest: null,
  attribution: null,
  isSearching: false,
  isDrawingBox: false,
  error: null,

  setBox: (box) => set({ box }),
  setDays: (days) => set({ days }),
  setMode: (mode) => set({ mode }),
  setOpacity: (opacity) => set({ opacity }),
  setSplitPosition: (splitPosition) => set({ splitPosition }),
  setSlotA: (slotA) => set({ slotA }),
  setSlotB: (slotB) => set({ slotB }),
  setCandidates: (candidates, tileTemplates, rankedLatest, attribution) =>
    set({ candidates, tileTemplates, rankedLatest, attribution }),
  setIsSearching: (isSearching) => set({ isSearching }),
  setIsDrawingBox: (isDrawingBox) => set({ isDrawingBox }),
  setError: (error) => set({ error }),
  clear: () =>
    set({
      box: null,
      slotA: null,
      slotB: null,
      candidates: [],
      tileTemplates: {},
      rankedLatest: null,
      attribution: null,
      isSearching: false,
      isDrawingBox: false,
      error: null,
    }),
}));
