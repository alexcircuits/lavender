import { useSyncExternalStore } from 'react';
import { DEFAULT_LOCALE, normalizeLocale, type SupportedLocale } from './locale';

export * from './locale';

export type TranslationCatalog = Record<string, string | Record<string, string>>;

const loadedCatalogs = new Map<SupportedLocale, TranslationCatalog>();
let currentLocale: SupportedLocale = DEFAULT_LOCALE;

const listeners = new Set<() => void>();
const cache = new Map<string, unknown>();

export function _clearCatalogCache(): void {
  loadedCatalogs.clear();
  currentLocale = DEFAULT_LOCALE;
  cache.clear();
}

export function subscribeLocale(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function notifyLocaleChange(): void {
  for (const l of listeners) l();
}

export function getCatalog(locale: string): TranslationCatalog | null {
  const norm = normalizeLocale(locale);
  return norm && norm !== DEFAULT_LOCALE ? (loadedCatalogs.get(norm) ?? null) : null;
}

export function getLocale(): SupportedLocale {
  return currentLocale;
}

export async function loadCatalog(locale: SupportedLocale): Promise<TranslationCatalog | null> {
  if (locale === DEFAULT_LOCALE) return null;
  const existing = loadedCatalogs.get(locale);
  if (existing) return existing;

  try {
    const mod =
      locale === 'es'
        ? await import('./locales/es.json')
        : locale === 'ru'
          ? await import('./locales/ru.json')
          : locale === 'zh-CN'
            ? await import('./locales/zh-CN.json')
            : null;
    if (!mod) return null;
    const cat = mod.default as TranslationCatalog;
    loadedCatalogs.set(locale, cat);
    return cat;
  } catch {
    return null;
  }
}

export async function setLocale(locale: string): Promise<SupportedLocale> {
  const normalized = normalizeLocale(locale) || DEFAULT_LOCALE;
  if (normalized !== DEFAULT_LOCALE) {
    await loadCatalog(normalized);
  }
  if (normalized !== currentLocale) {
    currentLocale = normalized;
    cache.clear();
    notifyLocaleChange();
  }
  if (typeof document !== 'undefined' && document.documentElement) {
    document.documentElement.lang = normalized;
  }
  return currentLocale;
}

export const loadLocale = setLocale;

const PLACEHOLDER_PATTERN = /\{([A-Za-z0-9_]+)\}/g;

function interpolate(pattern: string, values?: Record<string, unknown>): string {
  if (!values) return pattern;
  return pattern.replace(PLACEHOLDER_PATTERN, (lit, key) =>
    Object.hasOwn(values, key) ? String(values[key]) : lit,
  );
}

const filled = (v: unknown): v is string => typeof v === 'string' && v.trim() !== '';

function lookup(key: string): string | Record<string, string> | undefined {
  const cat = loadedCatalogs.get(currentLocale);
  if (!cat) return undefined;
  const entry = Object.hasOwn(cat, key) ? cat[key] : undefined;
  return filled(entry) || (entry && typeof entry === 'object') ? entry : undefined;
}

export function t(text: string, values?: Record<string, unknown> & { context?: string }): string {
  const entry = lookup(values?.context ? `${values.context}|${text}` : text);
  return interpolate(typeof entry === 'string' ? entry : text, values);
}

function pluralRules(locale: string): Intl.PluralRules {
  const k = `p|${locale}`;
  let r = cache.get(k) as Intl.PluralRules | undefined;
  if (!r) {
    r = new Intl.PluralRules(locale);
    cache.set(k, r);
  }
  return r;
}

export function tn(
  count: number,
  one: string,
  other: string,
  values?: Record<string, unknown>,
): string {
  const entry = lookup(other);
  const forms = entry && typeof entry === 'object' ? (entry as Record<string, string>) : null;
  const cat = pluralRules(currentLocale).select(count);
  const pattern =
    (forms && (filled(forms[cat]) ? forms[cat] : forms.other)) ||
    (pluralRules(DEFAULT_LOCALE).select(count) === 'one' ? one : other);
  return interpolate(pattern, { count, ...values });
}

export function translateKnown(text: string): string {
  const entry = loadedCatalogs.get(currentLocale)?.[text];
  return filled(entry) && typeof entry === 'string' ? entry : text;
}

export function msg(text: string): string {
  return text;
}

function cachedFormatter<T extends Intl.NumberFormat | Intl.DateTimeFormat, O>(
  kind: string,
  create: (locale: string, options?: O) => T,
  options?: O,
): T {
  const key = `${kind}|${currentLocale}|${options ? JSON.stringify(options) : ''}`;
  let f = cache.get(key) as T | undefined;
  if (!f) {
    f = create(currentLocale, options);
    cache.set(key, f);
  }
  return f;
}

export function formatNumber(value: number, options?: Intl.NumberFormatOptions): string {
  return cachedFormatter('n', (loc, opt) => new Intl.NumberFormat(loc, opt), options).format(value);
}

export function formatDateTime(
  value: Date | number | string,
  options?: Intl.DateTimeFormatOptions,
): string {
  return cachedFormatter('d', (loc, opt) => new Intl.DateTimeFormat(loc, opt), options).format(
    value instanceof Date ? value : new Date(value),
  );
}

export const formatDate = formatDateTime;

export function useI18n() {
  const locale = useSyncExternalStore(subscribeLocale, getLocale, getLocale);
  return { locale, t, tn, formatNumber, formatDateTime, formatDate, setLocale };
}
