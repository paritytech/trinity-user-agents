import { combineLatest, map, of } from 'rxjs';

import { createStreamResource } from '@/shared/resource';

import { DEFAULT_THEME_NAME, DEFAULT_THEME_PREFERENCE } from './constants';
import { themeRepository } from './repository';
import { themeService } from './service';
import { type Theme, type ThemeName, type ThemePreference } from './types';

/**
 * The theme before the stored row has been read back.
 *
 * IndexedDB is async, so the first paint gets the defaults resolved against the OS
 * rather than a stale guess at what the user picked. Exported for `hooks.ts`, whose
 * `useRead` needs the same value.
 */
export function defaultTheme(): Theme {
  return themeService.buildTheme(
    { preference: DEFAULT_THEME_PREFERENCE, name: DEFAULT_THEME_NAME },
    themeRepository.readSystemVariant(),
  );
}

/**
 * The theme the app is showing, live.
 *
 * One resource rather than one per setting: the two settings share a row and a
 * consumer that wants the variant almost always wants the name too. `liveQuery`
 * re-emits after every write, so a writer never has to tell anyone it wrote.
 */
export const themeResource = createStreamResource()
  .subscribe<Theme>(() =>
    combineLatest([themeRepository.settings$(), themeRepository.systemVariant$()]).pipe(
      map(([settings, systemVariant]) => themeService.buildTheme(settings, systemVariant)),
    ),
  )
  .mock(() => of(defaultTheme()))
  .cache<Theme>({
    initial: defaultTheme(),
    map(_, theme) {
      return theme;
    },
  })
  .build();

/**
 * Writes to the stored theme. Nothing is announced afterwards: the resource observes
 * the table, so the write itself is the notification.
 */
export function saveThemePreference(preference: ThemePreference): Promise<void> {
  return themeRepository.writeSettings({ preference });
}

export function saveThemeName(name: ThemeName): Promise<void> {
  return themeRepository.writeSettings({ name });
}
