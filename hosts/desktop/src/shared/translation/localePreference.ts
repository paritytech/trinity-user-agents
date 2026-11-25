import { useEffect, useState } from 'react';
import { type Observable, defer, distinctUntilChanged, fromEvent, map, merge, startWith } from 'rxjs';

import { DEFAULT_LOCALE, LOCALE_IDS } from './constants';
import { type Locale } from './types';

const LOCALE_STORAGE_KEY = 'polkadot_locale';
const LOCALE_CHANGE_EVENT = 'polkadot-locale-change';

function isSupportedLocale(value: string | null): value is Locale {
  return value !== null && LOCALE_IDS.some(id => id === value);
}

export function readLocale(): Locale {
  try {
    const saved = localStorage.getItem(LOCALE_STORAGE_KEY);

    return isSupportedLocale(saved) ? saved : DEFAULT_LOCALE;
  } catch {
    return DEFAULT_LOCALE;
  }
}

export function saveLocale(locale: Locale) {
  try {
    localStorage.setItem(LOCALE_STORAGE_KEY, locale);
  } catch {
    // Storage unavailable. localStorage is the only source of truth, so the selection
    // does not take effect and the radio snaps back — same as saveTheme in the same
    // situation.
  }
  window.dispatchEvent(new CustomEvent(LOCALE_CHANGE_EVENT));
}

/** The selected locale, now and on every change — the non-React twin of `useLocalePreference`. */
export function watchLocale(): Observable<Locale> {
  return defer(() =>
    merge(fromEvent(window, LOCALE_CHANGE_EVENT), fromEvent(window, 'storage')).pipe(
      map(() => readLocale()),
      startWith(readLocale()),
      distinctUntilChanged(),
    ),
  );
}

export const useLocalePreference = (): Locale => {
  const [locale, setLocale] = useState<Locale>(readLocale);

  useEffect(() => {
    const subscription = watchLocale().subscribe(setLocale);

    return () => subscription.unsubscribe();
  }, []);

  return locale;
};
