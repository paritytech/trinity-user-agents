import { Observable, map, startWith } from 'rxjs';
import * as v from 'valibot';

import { type ThemeSettingsRow, appDatabase, database, streamTable } from '@/shared/database';

import { DEFAULT_THEME_NAME, DEFAULT_THEME_PREFERENCE } from './constants';
import { themeNameSchema, themePreferenceSchema } from './schemas';
import { type ResolvedTheme, type ThemeSettings } from './types';

// The store holds exactly one row: there is one theme per install.
const THEME_SETTINGS_ID = 'main';

const table = database.themeSettings;

function parseSettings(row: ThemeSettingsRow | undefined): ThemeSettings {
  const preference = v.safeParse(themePreferenceSchema, row?.preference);
  const name = v.safeParse(themeNameSchema, row?.name);

  return {
    preference: preference.success ? preference.output : DEFAULT_THEME_PREFERENCE,
    name: name.success ? name.output : DEFAULT_THEME_NAME,
  };
}

function readSettings(): Promise<ThemeSettings> {
  return table.get(THEME_SETTINGS_ID).then(parseSettings);
}

/**
 * Patch one setting, leaving the other as it is: the two live in one row.
 *
 * In a transaction because it is a read-modify-write. Picking a palette and a mode in
 * quick succession would otherwise have both writes read the same pre-write row, and
 * the second `put` would drop the first one's field.
 */
function writeSettings(patch: Partial<ThemeSettings>): Promise<void> {
  return appDatabase.transaction('rw', table, async () => {
    const current = await readSettings();

    await table.put({ id: THEME_SETTINGS_ID, ...current, ...patch });
  });
}

function settings$(): Observable<ThemeSettings> {
  return streamTable(table, t => t.get(THEME_SETTINGS_ID)).pipe(map(parseSettings));
}

function readSystemVariant(): ResolvedTheme {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return 'light';

  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

/**
 * What the OS is currently set to, and every time it changes.
 *
 * The only listener left in this module, and it is not an app signal: `matchMedia`
 * is how the OS reports its own setting, so there is nothing to call instead.
 */
function systemVariant$(): Observable<ResolvedTheme> {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
    return new Observable<ResolvedTheme>(subscriber => {
      subscriber.next('light');
    });
  }

  const query = window.matchMedia('(prefers-color-scheme: dark)');

  return new Observable<ResolvedTheme>(subscriber => {
    const emit = () => subscriber.next(query.matches ? 'dark' : 'light');
    query.addEventListener('change', emit);

    return () => query.removeEventListener('change', emit);
  }).pipe(startWith(readSystemVariant()));
}

export const themeRepository = {
  readSettings,
  writeSettings,
  settings$,
  readSystemVariant,
  systemVariant$,
};
