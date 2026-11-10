import { useTheme } from '@novasamatech/tr-ui';
import { useEffect } from 'react';

import { useThemeVariant } from '@/domains/application';

export const ThemeSyncer = () => {
  const { setMode } = useTheme();
  const theme = useThemeVariant();

  useEffect(() => {
    setMode(theme);
    // Drive the host renderer's native UI (scrollbars, default form controls) off the
    // RESOLVED app theme directly. This works in both builds: in Electron nativeTheme
    // already aligns prefers-color-scheme, but the web build has no nativeTheme, so a
    // static `color-scheme: light dark` would let native UI follow the OS and diverge
    // from the chosen app theme. Setting the used color-scheme explicitly keeps it bound
    // to the app theme everywhere.
    document.documentElement.style.colorScheme = theme;
  }, [theme, setMode]);

  return null;
};
