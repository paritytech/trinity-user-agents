import { type Locale, type SupportedLocale } from './types';

export const DEFAULT_LOCALE: Locale = 'en';

// Order mirrors the Language settings screen in Figma (node 149-2286).
// `nativeName` is the endonym and is never translated; the English display name is
// a translated string keyed `feature.languageSettings.locale.<id>`.
export const SUPPORTED_LOCALES: SupportedLocale[] = [
  { id: 'en', nativeName: 'English' },
  { id: 'es', nativeName: 'Español' },
  { id: 'fr', nativeName: 'Français' },
  { id: 'de', nativeName: 'Deutsch' },
  { id: 'it', nativeName: 'Italiano' },
  { id: 'pt', nativeName: 'Português' },
  { id: 'zh-Hans', nativeName: '中文 (简体)' },
  { id: 'ja', nativeName: '日本語' },
  { id: 'ko', nativeName: '한국어' },
  { id: 'ru', nativeName: 'Русский' },
  { id: 'ar', nativeName: 'العربية' },
  { id: 'hi', nativeName: 'हिन्दी' },
  { id: 'tr', nativeName: 'Türkçe' },
];

export const LOCALE_IDS: Locale[] = SUPPORTED_LOCALES.map(locale => locale.id);

// Arabic is the only right-to-left locale in SUPPORTED_LOCALES. Adding he/fa/ur later means adding
// the id here as well as to the catalog above — utils.spec.ts fails if a locale has no direction.
export const RTL_LOCALES: Locale[] = ['ar'];
