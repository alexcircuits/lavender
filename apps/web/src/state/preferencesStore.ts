import { createStore } from 'zustand';

export type CoordinateFormat = 'dd' | 'dms' | 'mgrs';
export type UnitSystem = 'nautical' | 'metric' | 'imperial';
export type MotionPreference = 'system' | 'reduced' | 'full';
export type VisualPreset = 'normal' | 'noir' | 'thermal' | 'nvg' | 'crt';

export interface UserPreferences {
  readonly coordinateFormat: CoordinateFormat;
  readonly unitSystem: UnitSystem;
  readonly motion: MotionPreference;
  readonly labelArbitration: boolean;
  readonly defaultImagery: string | null;
  readonly visualPreset: VisualPreset;
  readonly hudEnabled: boolean;
  readonly scopeMaskEnabled: boolean;
  readonly sonarEnabled: boolean;
}

export const DEFAULT_USER_PREFERENCES: UserPreferences = {
  coordinateFormat: 'dd',
  unitSystem: 'metric',
  motion: 'system',
  labelArbitration: true,
  defaultImagery: null,
  visualPreset: 'normal',
  hudEnabled: false,
  scopeMaskEnabled: false,
  sonarEnabled: false,
};

export interface PreferencesState extends UserPreferences {
  setCoordinateFormat(coordinateFormat: CoordinateFormat): void;
  setUnitSystem(unitSystem: UnitSystem): void;
  setMotion(motion: MotionPreference): void;
  setLabelArbitration(labelArbitration: boolean): void;
  setDefaultImagery(defaultImagery: string | null): void;
  setVisualPreset(visualPreset: VisualPreset): void;
  setHudEnabled(hudEnabled: boolean): void;
  setScopeMaskEnabled(scopeMaskEnabled: boolean): void;
  setSonarEnabled(sonarEnabled: boolean): void;
  hydrate(prefs: Partial<UserPreferences>): void;
}

export const createPreferencesStore = () =>
  createStore<PreferencesState>()((set) => ({
    ...DEFAULT_USER_PREFERENCES,
    setCoordinateFormat: (coordinateFormat) => set({ coordinateFormat }),
    setUnitSystem: (unitSystem) => set({ unitSystem }),
    setMotion: (motion) => set({ motion }),
    setLabelArbitration: (labelArbitration) => set({ labelArbitration }),
    setDefaultImagery: (defaultImagery) => set({ defaultImagery }),
    setVisualPreset: (visualPreset) => set({ visualPreset }),
    setHudEnabled: (hudEnabled) => set({ hudEnabled }),
    setScopeMaskEnabled: (scopeMaskEnabled) => set({ scopeMaskEnabled }),
    setSonarEnabled: (sonarEnabled) => set({ sonarEnabled }),
    hydrate: (prefs) => set((s) => ({ ...s, ...prefs })),
  }));

export type PreferencesStore = ReturnType<typeof createPreferencesStore>;
