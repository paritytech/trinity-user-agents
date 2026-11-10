import { useEffect } from 'react';

import { useThemePreference } from '@/domains/application';

/**
 * Reflects the app theme PREFERENCE into Electron's app-global
 * nativeTheme.themeSource (via the preload bridge), so guest webviews inherit
 * the native color-scheme. Renders nothing; mounted once next to ThemeSyncer.
 *
 * Sends the preference (not the resolved light/dark) so 'system' stays dynamic.
 * In the web build window.App is absent — the optional call is a no-op.
 */
export const NativeThemeSyncer = () => {
  const preference = useThemePreference();

  useEffect(() => {
    window.App?.setNativeTheme?.(preference);
  }, [preference]);

  return null;
};
