import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import {
  _clearCatalogCache,
  applyDocumentLanguage,
  detectBrowserLocale,
  formatDate,
  formatDateTime,
  formatNumber,
  getCatalog,
  getLocale,
  loadLocale,
  msg,
  normalizeLocale,
  readStoredLocale,
  resolveLocale,
  setLocale,
  t,
  tn,
  writeStoredLocale,
} from './index';

describe('i18n subsystem', () => {
  beforeEach(() => {
    _clearCatalogCache();
  });

  afterEach(() => {
    _clearCatalogCache();
  });

  describe('locale resolution and normalization', () => {
    it('normalizes valid tags and folds region subtags', () => {
      expect(normalizeLocale('ru-RU')).toBe('ru');
      expect(normalizeLocale(' RU ')).toBe('ru');
      expect(normalizeLocale('en_GB')).toBe('en');
      expect(normalizeLocale('es-ES')).toBe('es');
      expect(normalizeLocale('xx')).toBeNull();
      expect(normalizeLocale(null)).toBeNull();
      expect(normalizeLocale(undefined)).toBeNull();
    });

    it('handles Chinese variants correctly (Simplified zh-CN, no zh-TW)', () => {
      expect(normalizeLocale('zh')).toBe('zh-CN');
      expect(normalizeLocale('zh-CN')).toBe('zh-CN');
      expect(normalizeLocale('zh-Hans-SG')).toBe('zh-CN');
      expect(normalizeLocale('zh-TW')).toBeNull();
      expect(normalizeLocale('zh-Hant-HK')).toBeNull();
    });

    it('detects locale from browser languages', () => {
      expect(detectBrowserLocale(['fr', 'ru-RU', 'en'])).toBe('ru');
      expect(detectBrowserLocale(['de-DE', 'es'])).toBe('es');
      expect(detectBrowserLocale(['ja', 'ko'])).toBe('en');
    });

    it('resolves boot locale with priority: query -> storage -> browser -> default', () => {
      const storage = {
        data: new Map<string, string>(),
        getItem(key: string) {
          return this.data.get(key) ?? null;
        },
        setItem(key: string, value: string) {
          this.data.set(key, value);
        },
      };

      // 1. Query param wins
      expect(
        resolveLocale({
          location: { search: '?lang=ru' },
          storage,
          languages: ['es'],
        }),
      ).toBe('ru');

      // 2. Storage wins when no query
      storage.setItem('gev:locale:v1', 'es');
      expect(
        resolveLocale({
          location: { search: '' },
          storage,
          languages: ['ru'],
        }),
      ).toBe('es');

      // 3. Browser languages when no query and no storage
      storage.data.clear();
      expect(
        resolveLocale({
          location: { search: '' },
          storage,
          languages: ['zh-Hans-CN', 'en'],
        }),
      ).toBe('zh-CN');

      // 4. Default fallback when nothing matches
      expect(
        resolveLocale({
          location: { search: '' },
          storage,
          languages: ['nl', 'de'],
        }),
      ).toBe('en');
    });

    it('safely handles throwing storage', () => {
      const brokenStorage = {
        getItem() {
          throw new Error('Storage denied');
        },
        setItem() {
          throw new Error('Storage denied');
        },
      };
      expect(readStoredLocale(brokenStorage)).toBeNull();
      expect(writeStoredLocale('ru', brokenStorage)).toBe(false);
      expect(writeStoredLocale(brokenStorage, 'ru')).toBe(false);
    });

    it('applyDocumentLanguage sets html attributes correctly', () => {
      const doc = { documentElement: { lang: '', dir: '' } };
      expect(applyDocumentLanguage(doc as unknown as Document, 'ru-RU')).toBe(true);
      expect(doc.documentElement.lang).toBe('ru');
      expect(doc.documentElement.dir).toBe('ltr');
    });
  });

  describe('basic translation & English pass-through', () => {
    it('passes English through directly with zero catalog overhead', () => {
      expect(getLocale()).toBe('en');
      expect(getCatalog('en')).toBeNull();
      expect(t('POWERED UP')).toBe('POWERED UP');
      expect(t('Non-existent string')).toBe('Non-existent string');
    });

    it('translates known strings when non-English locale is active', async () => {
      await setLocale('ru');
      expect(getLocale()).toBe('ru');
      expect(t('POWERED UP')).toBe('ЗАПУЩЕНО');
      expect(t('Save failed ({status}).', { status: '500' })).toBe('Не удалось сохранить (500).');

      await setLocale('es');
      expect(getLocale()).toBe('es');
      expect(t('POWERED UP')).toBe('ENCENDIDO');

      await setLocale('zh-CN');
      expect(getLocale()).toBe('zh-CN');
      expect(t('POWERED UP')).toBe('已增强');
    });

    it('falls back to English when translation key is missing in active catalog', async () => {
      await setLocale('ru');
      expect(t('A non-translated source string')).toBe('A non-translated source string');
    });

    it('msg() marks string and returns text unchanged', () => {
      expect(msg('Live Flights')).toBe('Live Flights');
      expect(msg('Camera {id}')).toBe('Camera {id}');
    });
  });

  describe('named placeholder interpolation', () => {
    it('interpolates named values into translation string', () => {
      expect(t('Save failed: {detail}', { detail: 'disk full' })).toBe('Save failed: disk full');

      expect(t('Sector {lat} {lon}', { lat: '45N', lon: '12E' })).toBe('Sector 45N 12E');
    });

    it('leaves missing placeholders literal for developer visibility', () => {
      expect(t('Hello {who} {missing}', { who: 'pilot' })).toBe('Hello pilot {missing}');
    });

    it('interpolates in translated language catalogs', async () => {
      await setLocale('ru');
      expect(t("Remove {title} from this app's saved keys", { title: 'FIRMS' })).toBe(
        'Удалить FIRMS из сохранённых ключей этого приложения',
      );

      await setLocale('zh-CN');
      expect(t('SECTOR {lat} {lon}', { lat: '45N', lon: '12E' })).toBe('扇区 45N 12E');
    });
  });

  describe('plural forms (tn)', () => {
    it('selects singular and plural forms in English', () => {
      expect(tn(1, '{count} camera', '{count} cameras')).toBe('1 camera');
      expect(tn(0, '{count} camera', '{count} cameras')).toBe('0 cameras');
      expect(tn(5, '{count} camera', '{count} cameras')).toBe('5 cameras');
    });

    it('selects one/few/many/other categories in Russian', async () => {
      await setLocale('ru');
      const msgPlural = (count: number) =>
        tn(count, 'POWER UP · {count} KEY WAITING', 'POWER UP · {count} KEYS WAITING');

      // Russian: 1 -> one (КЛЮЧ)
      expect(msgPlural(1)).toBe('ЗАПУСК · 1 КЛЮЧ В ОЖИДАНИИ');
      expect(msgPlural(21)).toBe('ЗАПУСК · 21 КЛЮЧ В ОЖИДАНИИ');

      // Russian: 2, 3, 4 -> few (КЛЮЧА)
      expect(msgPlural(2)).toBe('ЗАПУСК · 2 КЛЮЧА В ОЖИДАНИИ');
      expect(msgPlural(3)).toBe('ЗАПУСК · 3 КЛЮЧА В ОЖИДАНИИ');
      expect(msgPlural(24)).toBe('ЗАПУСК · 24 КЛЮЧА В ОЖИДАНИИ');

      // Russian: 5..20 -> many (КЛЮЧЕЙ)
      expect(msgPlural(5)).toBe('ЗАПУСК · 5 КЛЮЧЕЙ В ОЖИДАНИИ');
      expect(msgPlural(11)).toBe('ЗАПУСК · 11 КЛЮЧЕЙ В ОЖИДАНИИ');
      expect(msgPlural(20)).toBe('ЗАПУСК · 20 КЛЮЧЕЙ В ОЖИДАНИИ');
    });

    it('selects plural forms in Spanish (one vs other)', async () => {
      await setLocale('es');
      const msgPlural = (count: number) =>
        tn(count, 'POWER UP · {count} KEY WAITING', 'POWER UP · {count} KEYS WAITING');

      expect(msgPlural(1)).toBe('ENCENDER · 1 CLAVE EN ESPERA');
      expect(msgPlural(2)).toBe('ENCENDER · 2 CLAVES EN ESPERA');
    });

    it('selects plural forms in Chinese (other category)', async () => {
      await setLocale('zh-CN');
      const msgPlural = (count: number) =>
        tn(count, 'POWER UP · {count} KEY WAITING', 'POWER UP · {count} KEYS WAITING');

      expect(msgPlural(1)).toBe('增强 · 1 个密钥待配置');
      expect(msgPlural(5)).toBe('增强 · 5 个密钥待配置');
    });

    it('supports custom formatted count values in plural messages', () => {
      const formatted = (n: number) =>
        tn(n, '{count} map object', '{count} map objects', {
          count: formatNumber(n),
        });
      expect(formatted(12345)).toBe('12,345 map objects');
      expect(formatted(1)).toBe('1 map object');
    });
  });

  describe('context disambiguation', () => {
    it('keeps identical English unchanged regardless of context', () => {
      expect(t('CLEAR')).toBe('CLEAR');
      expect(t('CLEAR', { context: 'action' })).toBe('CLEAR');
    });

    it('differentiates distinct meanings in catalogs by context', async () => {
      await setLocale('ru');
      // CLEAR weather condition vs action button
      expect(t('CLEAR')).toBe('ЯСНО');
      expect(t('CLEAR', { context: 'action' })).toBe('СБРОСИТЬ');
      expect(t('CLEAR', { context: 'action' })).not.toBe(t('CLEAR'));
    });
  });

  describe('dynamic catalog loading & loadLocale', () => {
    it('dynamically loads catalog on demand via loadLocale', async () => {
      expect(getCatalog('es')).toBeNull();
      const resolved = await loadLocale('es');
      expect(resolved).toBe('es');
      expect(getLocale()).toBe('es');
      expect(getCatalog('es')).not.toBeNull();
    });

    it('re-uses already loaded catalogs without error', async () => {
      await setLocale('ru');
      const cat1 = getCatalog('ru');
      await setLocale('ru');
      const cat2 = getCatalog('ru');
      expect(cat1).toBe(cat2);
    });
  });

  describe('number and date formatting', () => {
    it('formats numbers sensitive to active locale', async () => {
      expect(formatNumber(1234.5)).toBe('1,234.5');
      await setLocale('ru');
      // Russian number format uses non-breaking space (or regular space) and comma
      const ruFormatted = formatNumber(1234.5);
      expect(ruFormatted).toMatch(/1[\s\u00A0]234,5/);
    });

    it('formats date and time sensitive to active locale', async () => {
      const date = new Date(Date.UTC(2026, 0, 15, 12, 0, 0));
      const options: Intl.DateTimeFormatOptions = {
        year: 'numeric',
        month: 'long',
        day: 'numeric',
        timeZone: 'UTC',
      };

      expect(formatDateTime(date, options)).toBe('January 15, 2026');
      expect(formatDate(date, options)).toBe('January 15, 2026');

      await setLocale('ru');
      expect(formatDateTime(date, options)).toBe('15 января 2026 г.');

      await setLocale('es');
      expect(formatDateTime(date, options)).toBe('15 de enero de 2026');
    });
  });
});
