export const LOCALE_STORAGE_KEY = 'gev:locale:v1';
export const DEFAULT_LOCALE = 'en';
export const SUPPORTED_LOCALES = ['en', 'es', 'ru', 'zh-CN'] as const;
export type SupportedLocale = (typeof SUPPORTED_LOCALES)[number];
export const LOCALES = SUPPORTED_LOCALES;

export const LOCALE_NAMES: Readonly<Record<SupportedLocale, string>> = Object.freeze({
  en: 'English',
  es: 'Español',
  ru: 'Русский',
  'zh-CN': '简体中文',
});

export function normalizeLocale(candidate: unknown): SupportedLocale | null {
  if (typeof candidate !== 'string') return null;
  const [primary, ...rest] = candidate.trim().toLowerCase().split(/[-_]/);
  if (!primary) return null;
  if (primary === 'zh') {
    return rest.some((part) => ['hant', 'tw', 'hk', 'mo'].includes(part)) ? null : 'zh-CN';
  }
  return (SUPPORTED_LOCALES as readonly string[]).includes(primary)
    ? (primary as SupportedLocale)
    : null;
}

export function queryLocale(
  location: { search?: string } | Location = globalThis.location,
): SupportedLocale | null {
  try {
    return normalizeLocale(new URLSearchParams(location?.search).get('lang'));
  } catch {
    return null;
  }
}

export function readStoredLocale(
  storage?: Storage | { getItem(key: string): string | null },
): SupportedLocale | null {
  try {
    return normalizeLocale((storage ?? globalThis.localStorage)?.getItem(LOCALE_STORAGE_KEY));
  } catch {
    return null;
  }
}

export function writeStoredLocale(
  first: string | Storage | { setItem(key: string, value: string): void } | undefined,
  second?: string | Storage | { setItem(key: string, value: string): void },
): boolean {
  const [locale, storage] = typeof first === 'string' ? [first, second] : [second, first];
  try {
    const store = (storage ?? globalThis.localStorage) as Storage | undefined;
    if (typeof store?.setItem !== 'function') return false;
    store.setItem(LOCALE_STORAGE_KEY, normalizeLocale(locale) || DEFAULT_LOCALE);
    return true;
  } catch {
    return false;
  }
}

export function detectBrowserLocale(
  languages: readonly string[] = globalThis.navigator?.languages ?? [],
): SupportedLocale {
  for (const candidate of languages) {
    const normalized = normalizeLocale(candidate);
    if (normalized) return normalized;
  }
  return DEFAULT_LOCALE;
}

export function resolveLocale({
  location = globalThis.location,
  storage,
  languages = globalThis.navigator?.languages,
}: {
  location?: { search?: string } | Location;
  storage?: Storage | { getItem(key: string): string | null };
  languages?: readonly string[];
} = {}): SupportedLocale {
  return queryLocale(location) ?? readStoredLocale(storage) ?? detectBrowserLocale(languages);
}

export function applyDocumentLanguage(
  doc: Document | { documentElement?: { lang?: string; dir?: string } } = globalThis.document,
  locale: string = DEFAULT_LOCALE,
): boolean {
  const root = doc?.documentElement;
  if (!root) return false;
  root.lang = normalizeLocale(locale) || DEFAULT_LOCALE;
  root.dir = 'ltr';
  return true;
}
