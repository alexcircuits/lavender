import { createStore } from 'zustand';
import type { SupportedLocale } from '../i18n';

export type CoordinateFormat = 'dd' | 'dms' | 'mgrs';
export type UnitSystem = 'nautical' | 'metric' | 'imperial';
export type MotionPreference = 'system' | 'reduced' | 'full';
export type VisualPreset = 'normal' | 'noir' | 'thermal' | 'nvg' | 'crt';

export interface UserPreferences {
  readonly locale: SupportedLocale;
  readonly coordinateFormat: CoordinateFormat;
  readonly unitSystem: UnitSystem;
  readonly motion: MotionPreference;
  readonly labelArbitration: boolean;
  readonly defaultImagery: string | null;
  readonly visualPreset: VisualPreset;
  readonly hudEnabled: boolean;
  readonly scopeMaskEnabled: boolean;
  readonly sonarEnabled: boolean;
  readonly celestialRingEnabled: boolean;
  readonly cockpitCloudsEnabled: boolean;
  readonly starrySky: boolean;
  readonly bloomEnabled: boolean;
  readonly bloomIntensity: number;
}

export const DEFAULT_USER_PREFERENCES: UserPreferences = {
  locale: 'en',
  coordinateFormat: 'dd',
  unitSystem: 'metric',
  motion: 'system',
  labelArbitration: true,
  defaultImagery: null,
  visualPreset: 'normal',
  hudEnabled: false,
  scopeMaskEnabled: false,
  sonarEnabled: false,
  // Upstream mounts the ring disabled (applicationShell.js `enabled: false`).
  celestialRingEnabled: false,
  // Upstream cockpit clouds default off (cockpitCloudEffects.js reads an
  // opt-in storage key; a missing value is off).
  cockpitCloudsEnabled: false,
  starrySky: true,
  bloomEnabled: false,
  bloomIntensity: 100,
};

export interface PreferencesState extends UserPreferences {
  setLocale(locale: SupportedLocale): Promise<void>;
  setCoordinateFormat(coordinateFormat: CoordinateFormat): void;
  setUnitSystem(unitSystem: UnitSystem): void;
  setMotion(motion: MotionPreference): void;
  setLabelArbitration(labelArbitration: boolean): void;
  setDefaultImagery(defaultImagery: string | null): void;
  setVisualPreset(visualPreset: VisualPreset): void;
  setHudEnabled(hudEnabled: boolean): void;
  setScopeMaskEnabled(scopeMaskEnabled: boolean): void;
  setSonarEnabled(sonarEnabled: boolean): void;
  setCelestialRingEnabled(celestialRingEnabled: boolean): void;
  setCockpitCloudsEnabled(cockpitCloudsEnabled: boolean): void;
  setStarrySky(starrySky: boolean): void;
  setBloomEnabled(bloomEnabled: boolean): void;
  setBloomIntensity(bloomIntensity: number): void;
  hydrate(prefs: Partial<UserPreferences>): void;
}

export const createPreferencesStore = () =>
  createStore<PreferencesState>()((set) => ({
    ...DEFAULT_USER_PREFERENCES,
    setLocale: async (locale) => {
      const { setLocale: applyLocale } = await import('../i18n');
      await applyLocale(locale);
      set({ locale });
    },
    setCoordinateFormat: (coordinateFormat) => set({ coordinateFormat }),
    setUnitSystem: (unitSystem) => set({ unitSystem }),
    setMotion: (motion) => set({ motion }),
    setLabelArbitration: (labelArbitration) => set({ labelArbitration }),
    setDefaultImagery: (defaultImagery) => set({ defaultImagery }),
    setVisualPreset: (visualPreset) => set({ visualPreset }),
    setHudEnabled: (hudEnabled) => set({ hudEnabled }),
    setScopeMaskEnabled: (scopeMaskEnabled) => set({ scopeMaskEnabled }),
    setSonarEnabled: (sonarEnabled) => set({ sonarEnabled }),
    setCelestialRingEnabled: (celestialRingEnabled) => set({ celestialRingEnabled }),
    setCockpitCloudsEnabled: (cockpitCloudsEnabled) => set({ cockpitCloudsEnabled }),
    setStarrySky: (starrySky) => set({ starrySky }),
    setBloomEnabled: (bloomEnabled) => set({ bloomEnabled }),
    setBloomIntensity: (bloomIntensity) => set({ bloomIntensity }),
    hydrate: (prefs) => set((s) => ({ ...s, ...prefs })),
  }));

export type PreferencesStore = ReturnType<typeof createPreferencesStore>;
