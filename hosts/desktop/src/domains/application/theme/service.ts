import { type ResolvedTheme, type Theme, type ThemePreference, type ThemeSettings } from './types';

/**
 * What a preference means right now.
 *
 * `system` is the only value that needs the OS; the other two are already an answer,
 * which is why the preference is kept alongside the resolved variant rather than
 * replaced by it.
 */
function resolveVariant(preference: ThemePreference, systemVariant: ResolvedTheme): ResolvedTheme {
  return preference === 'system' ? systemVariant : preference;
}

function buildTheme(settings: ThemeSettings, systemVariant: ResolvedTheme): Theme {
  return {
    preference: settings.preference,
    name: settings.name,
    variant: resolveVariant(settings.preference, systemVariant),
  };
}

export const themeService = {
  resolveVariant,
  buildTheme,
};
