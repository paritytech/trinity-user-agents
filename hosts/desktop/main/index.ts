import 'source-map-support/register';

import { version as appVersion } from '~config';
import { electronProtocol } from '~config';
import { BrowserWindow, app, ipcMain, net, protocol, shell, systemPreferences, webContents } from 'electron';
import { REACT_DEVELOPER_TOOLS, installExtension } from 'electron-devtools-installer';

import { type CallWindowLaunch } from '@/shared/call-bridge';
import { type OsDevicePermissionStatus } from '@/domains/product';

import { createCallWindow } from './factories/callWindow';
import { setupAppCrashMonitoring, setupWindowCrashRecovery } from './factories/crash-recovery';
import { runAppSingleInstance } from './factories/instance';
import { setupLogExport, setupLogger } from './factories/logs';
import { setupPowerMonitor } from './factories/power-monitor';
import {
  extractDeepLinkFromArgv,
  onRendererReady,
  processDeepLink,
  registerDeepLinkProtocol,
  setupDeepLinkHandlers,
} from './factories/protocol';
import { setupNativeTheme } from './factories/theme';
import { setAutoUpdaterMainWindow, setupAutoUpdater } from './factories/updater';
import { createWindow } from './factories/window';
import { setupNotifications } from './notifications';
import { setupSandbox } from './sandbox';
import { registerAutoUpdateSupportedSyncIpc } from './shared/auto-update';
import { ENVIRONMENT } from './shared/constants/environment';
import { PLATFORM } from './shared/constants/platform';
import { getOsType } from './shared/lib/utils';
import { type ProxyFetchRequest, type ProxyFetchResponse } from './shared/proxyFetch';
import { initSentry } from './shared/sentry';
import { registerUpdateChannelSyncIpc } from './shared/update-channel';

// Public URL to the Polkadot Desktop app icon, served from the community mirror
// on `main`. Sent as the `HostIcon` entry in the SSO handshake proposal so the
// paired mobile client can render it. Permanent path (no commit SHA) so the icon
// can be swapped centrally without shipping new desktop builds.
const HOST_ICON_URL =
  'https://raw.githubusercontent.com/paritytech/polkadot-desktop-community/main/src/shared/assets/images/polkadot-desktop-icon.png';

// The one OS gate this host applies: macOS camera/mic. Everything else is not gated
// by the OS as far as the app is concerned, so both the prompt and the status
// handler answer "not applicable" from the same predicate.
function osGatedMediaType(permission: string): 'camera' | 'microphone' | null {
  if (process.platform !== 'darwin') return null;
  if (permission === 'Camera') return 'camera';
  if (permission === 'Microphone') return 'microphone';

  return null;
}

// Applied before requestSingleInstanceLock — the lock is keyed on userData, so
// isolating the userData path per e2e instance also gives each instance its own lock.
if (process.env['ELECTRON_USER_DATA']) {
  app.setPath('userData', process.env['ELECTRON_USER_DATA']);
}

runAppSingleInstance(async () => {
  initSentry();
  registerUpdateChannelSyncIpc();
  registerAutoUpdateSupportedSyncIpc();
  setupLogger();
  setupSandbox(() => mainWindow?.webContents ?? null);
  setupAppCrashMonitoring();
  registerDeepLinkProtocol();

  // Register OS-level deep-link handlers before app.whenReady() so no event
  // is missed. The getter is a closure over `mainWindow` declared below.
  setupDeepLinkHandlers(() => mainWindow);

  protocol.registerSchemesAsPrivileged([
    {
      scheme: electronProtocol,
      privileges: {
        standard: true, // Enables URL parsing like http/https
        secure: true, // Treated as secure context
        supportFetchAPI: true,
        corsEnabled: true,
      },
    },
  ]);

  app.commandLine.appendSwitch('force-color-profile', 'srgb');
  app.commandLine.appendSwitch('autoplay-policy', 'no-user-gesture-required');
  if (ENVIRONMENT.IS_DEV) {
    app.commandLine.appendSwitch('ignore-certificate-errors');
  }

  if (PLATFORM.IS_LINUX) {
    app.disableHardwareAcceleration();
  }

  await app.whenReady();

  setupNativeTheme();

  let mainWindow: BrowserWindow | null = createWindow();
  // The single active call window (see the bridge's single-call TODO). Retained
  // so the renderer can focus it (return-to-call bar) and learn when it closes.
  let activeCallWindow: BrowserWindow | null = null;
  setupWindowCrashRecovery(mainWindow);
  setupPowerMonitor(mainWindow);
  setupAutoUpdater(mainWindow);

  // Windows / Linux cold start: the deep-link URL is already in process.argv.
  // processDeepLink will store the route for the renderer to pull on mount.
  if (PLATFORM.IS_WINDOWS || PLATFORM.IS_LINUX) {
    const url = extractDeepLinkFromArgv(process.argv);
    if (url) processDeepLink(url, mainWindow);
  }

  // The renderer signals when it has mounted and registered its protocol-open
  // listener. At that point any pending cold-start deeplink is flushed.
  ipcMain.on('deeplink-renderer-ready', () => {
    if (mainWindow) onRendererReady(mainWindow);
  });

  app.on('activate', () => {
    if (mainWindow === null) {
      mainWindow = createWindow();
      setupWindowCrashRecovery(mainWindow);
      setupPowerMonitor(mainWindow);
      setAutoUpdaterMainWindow(mainWindow);
    }
  });

  app.on('web-contents-created', (_, contents) =>
    contents.on('will-navigate', event => !ENVIRONMENT.IS_DEV && event.preventDefault()),
  );

  app.on('window-all-closed', () => {
    if (!PLATFORM.IS_MAC) {
      app.quit();
    }

    setAutoUpdaterMainWindow(null);
    mainWindow?.destroy();
    mainWindow = null;
  });

  setupLogExport();

  ipcMain.handle(
    'call:open',
    (_event, launchParams: { launch: CallWindowLaunch; iceConfig: { turnHost?: string; turnSecret?: string } }) => {
      if (!mainWindow) {
        console.warn('[call] call:open received but mainWindow is null — ignoring');
        return;
      }
      // A window that died without its 'closed' handler running would otherwise
      // block every later call.
      if (activeCallWindow?.isDestroyed()) activeCallWindow = null;
      // Only one call at a time: the bridge owns a single MessagePort, so a second
      // window would orphan the first — unreachable by call:focus, yet still firing
      // call:ended for a call that is still running.
      if (activeCallWindow) {
        console.warn('[call] call:open while a call is active — focusing the existing window');
        if (activeCallWindow.isMinimized()) activeCallWindow.restore();
        activeCallWindow.show();
        activeCallWindow.focus();

        return;
      }
      const callWindow = createCallWindow({ ...launchParams, mainWindow });
      activeCallWindow = callWindow;
      // When the call window closes (user ended, peer ended + terminal dwell, or
      // manual close), tell the renderer so it can drop the return-to-call bar.
      callWindow.on('closed', () => {
        if (activeCallWindow === callWindow) activeCallWindow = null;
        mainWindow?.webContents.send('call:ended');
      });
    },
  );

  ipcMain.on('call:focus', () => {
    if (!activeCallWindow) return;
    if (activeCallWindow.isMinimized()) activeCallWindow.restore();
    activeCallWindow.show();
    activeCallWindow.focus();
  });

  ipcMain.on('reload', () => mainWindow?.webContents.reload());
  ipcMain.on('window:close', () => mainWindow?.close());
  ipcMain.handle('window:get-fullscreen', event => {
    const window = BrowserWindow.fromWebContents(event.sender);
    return window?.isFullScreen() ?? false;
  });
  ipcMain.on('focusWindow', () => {
    if (!mainWindow) return;
    if (mainWindow.isMinimized()) mainWindow.restore();
    mainWindow.show();
    mainWindow.focus();
  });

  const preloadPath = app.getPath('temp');
  ipcMain.handle('getPreloadPath', () => preloadPath);

  setupNotifications({
    getMainWindow: () => mainWindow,
    ensureMainWindow: () => {
      if (mainWindow === null) {
        mainWindow = createWindow();
        setupWindowCrashRecovery(mainWindow);
        setupPowerMonitor(mainWindow);
        setAutoUpdaterMainWindow(mainWindow);
      }
      return mainWindow;
    },
  });

  ipcMain.handle('setBadgeCount', (_event, count: number) => {
    app.setBadgeCount(count);
  });

  ipcMain.handle(
    'requestSystemDevicePermission',
    async (_event, permission: 'Camera' | 'Microphone' | 'Bluetooth' | 'Location'): Promise<boolean> => {
      const mediaType = osGatedMediaType(permission);
      if (!mediaType) return true;

      const status = systemPreferences.getMediaAccessStatus(mediaType);
      if (status === 'granted') return true;
      if (status === 'denied' || status === 'restricted') return false;

      return systemPreferences.askForMediaAccess(mediaType);
    },
  );

  // Read-only twin of `requestSystemDevicePermission`: never prompts. The core revalidates
  // a stored device grant against this before answering a product, so a capability
  // revoked in System Settings stops reading as usable.
  ipcMain.handle('getSystemDevicePermissionStatus', (_event, permission: string): OsDevicePermissionStatus => {
    const mediaType = osGatedMediaType(permission);
    if (!mediaType) return 'not-applicable';

    switch (systemPreferences.getMediaAccessStatus(mediaType)) {
      case 'granted':
        return 'granted';
      case 'denied':
      case 'restricted':
        return 'denied';
      default:
        return 'not-determined';
    }
  });

  ipcMain.handle('openSystemPrivacySettings', async (_event, permission: 'Camera' | 'Microphone'): Promise<boolean> => {
    const url =
      process.platform === 'darwin'
        ? permission === 'Camera'
          ? 'x-apple.systempreferences:com.apple.preference.security?Privacy_Camera'
          : 'x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone'
        : process.platform === 'win32'
          ? permission === 'Camera'
            ? 'ms-settings:privacy-webcam'
            : 'ms-settings:privacy-microphone'
          : null;

    if (url === null) {
      return false;
    }

    try {
      await shell.openExternal(url);
      return true;
    } catch {
      return false;
    }
  });

  ipcMain.handle(
    'getHostMetadata',
    (): {
      hostName?: string;
      hostVersion?: string;
      hostIcon?: string;
      platformType?: string;
      platformVersion?: string;
    } => ({
      hostName: 'Polkadot Desktop',
      hostVersion: appVersion,
      // Public URL to the app icon, sent as the `HostIcon` metadata entry in the
      // SSO handshake proposal so the mobile client can show it on the pairing
      // screen. A URL (not base64) keeps the QR-encoded proposal small; the
      // asset is mirrored from this repo into the public community repo.
      hostIcon: HOST_ICON_URL,
      platformType: getOsType(),
      platformVersion: process.getSystemVersion(),
    }),
  );

  ipcMain.handle('getWebContentsMemory', (_event, webContentsId: number): number | null => {
    const target = webContents.fromId(webContentsId);
    if (!target || target.isDestroyed()) return null;

    let pid: number;
    try {
      pid = target.getOSProcessId();
    } catch {
      return null;
    }
    if (!pid) return null;

    const entry = app.getAppMetrics().find(m => m.pid === pid);
    if (!entry) return null;

    return entry.memory.workingSetSize * 1024;
  });

  // Performs a fetch in the main process so callers bypass renderer CORS. Used by push
  // notifications and by product workers (which gate the URL against the product's remote
  // permissions in the renderer before calling this). The sandbox never reaches this directly —
  // only trusted host-renderer code does.
  ipcMain.handle('proxyFetch', async (_event, req: ProxyFetchRequest): Promise<ProxyFetchResponse> => {
    const response = await net.fetch(req.url, {
      method: req.method,
      headers: req.headers,
      // Re-wrap into an ArrayBuffer-backed view so it satisfies BodyInit.
      body: req.body ? new Uint8Array(req.body) : undefined,
    });

    return {
      status: response.status,
      statusText: response.statusText,
      headers: [...response.headers],
      body: new Uint8Array(await response.arrayBuffer()),
      url: response.url,
      redirected: response.redirected,
    };
  });

  if (ENVIRONMENT.IS_DEV) {
    await installExtension(REACT_DEVELOPER_TOOLS);

    // Reloading extensions for correct initialization in dev tools
    // session.defaultSession.extensions.getAllExtensions().map(e => {
    //   session.defaultSession.extensions.loadExtension(e.path);
    // });
  }
});
