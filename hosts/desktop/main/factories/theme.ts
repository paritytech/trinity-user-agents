import { ipcMain, nativeTheme } from 'electron';

export type ThemeSource = 'system' | 'light' | 'dark';

/**
 * Narrow an untrusted IPC payload to a valid Electron theme source.
 * The equality checks narrow `unknown` to the literal union, so the return
 * value is correctly typed without an `as` assertion.
 */
export function parseThemeSource(value: unknown): ThemeSource | null {
  return value === 'system' || value === 'light' || value === 'dark' ? value : null;
}

let listenerRegistered = false;

/**
 * Bridges the renderer's app theme preference into Electron's app-global
 * `nativeTheme.themeSource`, so guest webviews inherit the native color-scheme
 * (scrollbars, default form controls) and follow theme toggles live.
 *
 * Idempotent: only the first call registers the ipc listener.
 */
export function setupNativeTheme(): void {
  if (listenerRegistered) return;
  listenerRegistered = true;

  ipcMain.on('app:set-native-theme', (_event, source: unknown) => {
    const parsed = parseThemeSource(source);
    if (parsed) nativeTheme.themeSource = parsed;
  });
}
