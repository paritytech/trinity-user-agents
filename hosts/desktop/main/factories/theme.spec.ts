import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('electron', () => ({
  ipcMain: { on: vi.fn() },
  nativeTheme: { themeSource: 'system' },
}));

import { parseThemeSource } from './theme';

describe('parseThemeSource', () => {
  it('accepts the three valid sources', () => {
    expect(parseThemeSource('system')).toBe('system');
    expect(parseThemeSource('light')).toBe('light');
    expect(parseThemeSource('dark')).toBe('dark');
  });

  it('rejects anything else', () => {
    expect(parseThemeSource('Dark')).toBeNull();
    expect(parseThemeSource('')).toBeNull();
    expect(parseThemeSource(undefined)).toBeNull();
    expect(parseThemeSource(42)).toBeNull();
    expect(parseThemeSource({ variant: 'dark' })).toBeNull();
  });
});

describe('setupNativeTheme', () => {
  // The factory keeps a module-level "registered" flag for idempotency, so each
  // test resets the module registry to start from a fresh, unregistered state.
  // The electron mock is re-created from its factory on each fresh import too,
  // giving per-test isolation of `ipcMain.on` and `nativeTheme.themeSource`.
  beforeEach(() => {
    vi.resetModules();
  });

  it('registers the ipc listener only once across repeated calls', async () => {
    const { ipcMain } = await import('electron');
    const { setupNativeTheme } = await import('./theme');

    setupNativeTheme();
    setupNativeTheme();

    expect(ipcMain.on).toHaveBeenCalledTimes(1);
    const firstCall = vi.mocked(ipcMain.on).mock.calls[0];
    if (!firstCall) throw new Error('listener was not registered');
    expect(firstCall[0]).toBe('app:set-native-theme');
  });

  it('sets nativeTheme.themeSource for a valid payload and ignores garbage', async () => {
    const { ipcMain, nativeTheme } = await import('electron');
    const { setupNativeTheme } = await import('./theme');

    setupNativeTheme();
    const firstCall = vi.mocked(ipcMain.on).mock.calls[0];
    if (!firstCall) throw new Error('listener was not registered');
    // The registered listener is typed for a real IpcMainEvent; in this unit we
    // only exercise the payload branch, so invoke it through a loose signature.
    // eslint-disable-next-line @typescript-eslint/consistent-type-assertions
    const handler = firstCall[1] as (event: unknown, source: unknown) => void;

    handler({}, 'dark');
    expect(nativeTheme.themeSource).toBe('dark');

    handler({}, 'nonsense');
    expect(nativeTheme.themeSource).toBe('dark'); // unchanged
  });
});
