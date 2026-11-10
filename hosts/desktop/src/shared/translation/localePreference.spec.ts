// @vitest-environment happy-dom
import { act, renderHook } from '@testing-library/react';
import { type Subscription } from 'rxjs';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { readLocale, saveLocale, useLocalePreference, watchLocale } from './localePreference';

const STORAGE_KEY = 'polkadot_locale';

afterEach(() => {
  vi.restoreAllMocks();
});

describe('readLocale', () => {
  it('defaults to en when nothing is stored', () => {
    expect(readLocale()).toBe('en');
  });

  it('returns a stored supported locale', () => {
    localStorage.setItem(STORAGE_KEY, 'ja');

    expect(readLocale()).toBe('ja');
  });

  it('falls back to en when the stored value is not a supported locale', () => {
    localStorage.setItem(STORAGE_KEY, 'klingon');

    expect(readLocale()).toBe('en');
  });

  it('falls back to en when storage throws', () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('storage unavailable');
    });

    expect(readLocale()).toBe('en');
  });
});

describe('saveLocale', () => {
  it('persists the locale', () => {
    saveLocale('fr');

    expect(localStorage.getItem(STORAGE_KEY)).toBe('fr');
  });

  it('does not throw when storage is unavailable', () => {
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('storage unavailable');
    });

    expect(() => saveLocale('fr')).not.toThrow();
  });
});

describe('useLocalePreference', () => {
  it('returns the persisted locale', () => {
    localStorage.setItem(STORAGE_KEY, 'de');

    const { result } = renderHook(() => useLocalePreference());

    expect(result.current).toBe('de');
  });

  it('re-reads when the locale changes elsewhere', () => {
    const { result } = renderHook(() => useLocalePreference());
    expect(result.current).toBe('en');

    act(() => saveLocale('ru'));

    expect(result.current).toBe('ru');
  });
});

describe('watchLocale', () => {
  it('emits the stored locale first', () => {
    localStorage.setItem(STORAGE_KEY, 'de');

    const emissions: string[] = [];
    const subscription: Subscription = watchLocale().subscribe(locale => emissions.push(locale));

    expect(emissions[0]).toBe('de');

    subscription.unsubscribe();
  });

  it('emits on saveLocale', () => {
    const emissions: string[] = [];
    const subscription: Subscription = watchLocale().subscribe(locale => emissions.push(locale));

    saveLocale('ru');

    expect(emissions).toEqual(['en', 'ru']);

    subscription.unsubscribe();
  });

  it('does not re-emit an unchanged value', () => {
    const emissions: string[] = [];
    const subscription: Subscription = watchLocale().subscribe(locale => emissions.push(locale));

    saveLocale('en');

    expect(emissions).toEqual(['en']);

    subscription.unsubscribe();
  });

  it('reads at subscribe time', () => {
    const locale$ = watchLocale();

    saveLocale('de');

    const emissions: string[] = [];
    const subscription: Subscription = locale$.subscribe(locale => emissions.push(locale));

    expect(emissions[0]).toBe('de');

    subscription.unsubscribe();
  });
});
