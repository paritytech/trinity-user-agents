/** What the user picked for light/dark: an explicit mode, or "follow the OS". */
export type ThemePreference = 'light' | 'dark' | 'system';

/** What that resolves to once the OS has been consulted. */
export type ResolvedTheme = 'light' | 'dark';

/** The named palette the UI kit renders with. Orthogonal to light/dark. */
export type ThemeName = 'berlin' | 'tokyo' | 'lisbon' | 'malta';

/** What the user picked, as persisted. The variant is derived, so it is not stored. */
export type ThemeSettings = {
  preference: ThemePreference;
  name: ThemeName;
};

/**
 * The complete theme the app is showing.
 *
 * `preference` and `variant` are both here because they answer different questions:
 * the settings control shows what the user chose (`system`), while everything that
 * paints needs what that means right now (`dark`).
 */
export type Theme = {
  preference: ThemePreference;
  variant: ResolvedTheme;
  name: ThemeName;
};
