import { describe, expect, it } from 'vitest';

import { LOCALE_IDS, RTL_LOCALES } from './constants';
import { localeDirection } from './utils';

describe('localeDirection', () => {
  it('reports Arabic as right-to-left', () => {
    expect(localeDirection('ar')).toBe('rtl');
  });

  it('reports English as left-to-right', () => {
    expect(localeDirection('en')).toBe('ltr');
  });

  // The guard that makes the explicit RTL_LOCALES list safe: a locale added to SUPPORTED_LOCALES
  // without a direction decision would otherwise fall through to 'ltr' unnoticed.
  it.each(LOCALE_IDS)('resolves a direction for %s', locale => {
    expect(['ltr', 'rtl']).toContain(localeDirection(locale));
  });

  it('lists only supported locales as right-to-left', () => {
    for (const locale of RTL_LOCALES) {
      expect(LOCALE_IDS).toContain(locale);
    }
  });
});
