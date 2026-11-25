import { DirectionProvider } from '@radix-ui/react-direction';
import { type ReactNode, useEffect, useState } from 'react';
import { IntlProvider } from 'react-intl';

import { DEFAULT_LOCALE } from './constants';
import englishMessages from './locales/en.json';
import { type FlatMessages, type Locale, type TranslationMessages } from './types';
import { flattenMessages, localeDirection } from './utils';

async function loadLocale(locale: Locale) {
  const loaded = await import(`./locales/${locale}.json`);

  // Vite resolves a JSON import to a module namespace whose keys include `default`
  // alongside the named exports; flattening the namespace itself would emit every
  // message twice, the second copy under a `default.` prefix.
  return loaded.default ?? loaded;
}

const englishFlatMessages = flattenMessages(englishMessages);

// react-intl resolves a missing key to the id, never to another catalog — `defaultLocale`
// only silences the warning. Layering each catalog over English is what makes an
// untranslated (or partially translated) locale render English instead of raw ids.
function withEnglishFallback(messages: TranslationMessages): FlatMessages {
  return { ...englishFlatMessages, ...flattenMessages(messages) };
}

// Locale and messages live in one state value so a render can never pair one locale's
// text with another locale's number and date formatting.
type Translation = {
  locale: Locale;
  messages: FlatMessages;
};

const ENGLISH: Translation = { locale: DEFAULT_LOCALE, messages: englishFlatMessages };

type TranslationProviderProps = {
  children: ReactNode;
  locale?: Locale;
};

export const TranslationProvider = ({ children, locale }: TranslationProviderProps) => {
  const [translation, setTranslation] = useState<Translation>(ENGLISH);

  useEffect(() => {
    if (!locale || locale === DEFAULT_LOCALE) {
      setTranslation(ENGLISH);

      return;
    }

    let cancelled = false;

    const loadMessages = async () => {
      try {
        const loaded = await loadLocale(locale);

        if (!cancelled) {
          setTranslation({ locale, messages: withEnglishFallback(loaded) });
        }
      } catch (error) {
        console.error(`Failed to load locale "${locale}":`, error);

        if (!cancelled) {
          setTranslation(ENGLISH);
        }
      }
    };

    loadMessages();

    return () => {
      cancelled = true;
    };
  }, [locale]);

  // Derived from the rendering locale, not the `locale` prop: during the catalog load the prop is
  // already the new locale while `translation` still holds the old one, so keying off the prop would
  // flip the document to RTL with English text still on screen.
  const direction = localeDirection(translation.locale);

  useEffect(() => {
    document.documentElement.dir = direction;
    // Screen readers take pronunciation from `lang`, and CSS `:lang()` selectors depend on it.
    document.documentElement.lang = translation.locale;
  }, [translation.locale, direction]);

  return (
    <DirectionProvider dir={direction}>
      <IntlProvider locale={translation.locale} messages={translation.messages} defaultLocale={DEFAULT_LOCALE}>
        {children}
      </IntlProvider>
    </DirectionProvider>
  );
};
