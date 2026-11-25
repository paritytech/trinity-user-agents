import { useAction, useRead } from '@/shared/hooks';

import { defaultTheme, saveThemeName, saveThemePreference, themeResource } from './resource';
import { type Theme } from './types';

/** The theme the app is showing, live: the user's preference, what it resolves to, and the palette. */
export const useTheme = (): Theme => {
  const { data } = useRead(themeResource, {
    params: {},
    defaultValue: defaultTheme(),
    map: cache => cache,
  });

  return data;
};

/** The resolved light/dark variant — what anything that paints actually needs. */
export const useThemeVariant = () => useTheme().variant;

/** The named palette. */
export const useThemeName = () => useTheme().name;

/** What the user chose, `system` included — for the control that shows the choice. */
export const useThemePreference = () => useTheme().preference;

/**
 * Writes go through `useAction` like any other mutation. They resolve once the row is
 * stored; the resource observes that table, so no caller has to thread the result
 * back into a read.
 */
export const useSetThemePreference = () => useAction(saveThemePreference);

export const useSetThemeName = () => useAction(saveThemeName);
