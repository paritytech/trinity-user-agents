// The closed set of locales shipped under ./locales. Narrow rather than `string` so a
// value that has no locale file is a type error instead of a failed dynamic import.
export type Locale = 'en' | 'es' | 'fr' | 'de' | 'it' | 'pt' | 'zh-Hans' | 'ja' | 'ko' | 'ru' | 'ar' | 'hi' | 'tr';

// Declared here rather than imported from @radix-ui/react-direction so this library's surface
// doesn't depend on Radix's types. The values match, so it stays assignable to DirectionProvider.
export type Direction = 'ltr' | 'rtl';

export type SupportedLocale = {
  id: Locale;
  nativeName: string;
};

export type TranslationMessages = Record<string, unknown>;

export type FlatMessages = Record<string, string>;

export type LocaleLoader = (locale: Locale) => Promise<TranslationMessages>;
