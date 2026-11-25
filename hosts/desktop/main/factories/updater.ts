import { type BrowserWindow, app, ipcMain } from 'electron';
import { default as Store } from 'electron-store';
import { autoUpdater } from 'electron-updater';

import { getAutoUpdateSupported } from '../shared/auto-update';
import { type UpdateChannel, AUTO_UPDATE_ENABLED, DEFAULT_UPDATE_CHANNEL, UPDATE_CHANNEL } from '../shared/constants/store';

// A static feed base URL, without the channel suffix the runtime appends. Set = this build reads
// updates from object storage rather than from GitHub Releases; see config/index.js.
const AUTO_UPDATE_URL = process.env['AUTO_UPDATE_URL'] ?? '';
const UPDATE_REPO_OWNER = process.env['UPDATE_REPO_OWNER'] ?? '';
const UPDATE_REPO_NAME = process.env['UPDATE_REPO_NAME'] ?? '';

// Persisted flag, kept under its original name so builds that already wrote it don't re-run the
// one-time reset below. The name records when the reset was introduced, not where updates come
// from; renaming it would replay the reset on every existing install for no benefit.
const MIGRATED_KEY = 'autoUpdateMigratedToS3';

let mainWindowRef: BrowserWindow | null = null;

function sendUpdateEvent(type: string, data?: unknown) {
  mainWindowRef?.webContents.send('app:update-event', { type, data });
}

export function setAutoUpdaterMainWindow(window: BrowserWindow | null) {
  mainWindowRef = window;
}

function normalizeChannel(raw: unknown): UpdateChannel {
  return raw === 'experimental' ? 'experimental' : 'stable';
}

export function setupAutoUpdater(mainWindow: BrowserWindow | null) {
  mainWindowRef = mainWindow;
  // Single gate shared with the menu and the renderer. Among other things it requires a configured
  // update repository — without one there is nothing to point the updater at.
  const isSupported = getAutoUpdateSupported();
  const store = new Store({
    defaults: {
      [AUTO_UPDATE_ENABLED]: isSupported,
      [UPDATE_CHANNEL]: DEFAULT_UPDATE_CHANNEL,
    },
  });

  // One-time migration: older builds persisted AUTO_UPDATE_ENABLED as `false` because their
  // build-time gate was off (the v0.6.21/22 IS_DEV regression).
  // Reset to `true` once this build actually supports auto-update.
  if (isSupported && !store.get(MIGRATED_KEY)) {
    store.set(AUTO_UPDATE_ENABLED, true);
    store.set(MIGRATED_KEY, true);
  }

  const ALLOWED_STORE_KEYS = new Set([AUTO_UPDATE_ENABLED, MIGRATED_KEY, UPDATE_CHANNEL]);

  // A channel means a different thing to each provider, so it is resolved per provider rather than
  // normalised into one shape.
  //
  // On a static feed it is a directory: `stable/` and `latest/` are two independently promoted
  // copies of the metadata, and linux arm64 keeps its own metadata file, named explicitly because
  // the generic provider does not derive an arch suffix on its own.
  //
  // On GitHub Releases it is a release flag: `stable` reads the release marked Latest (what
  // promote-to-stable sets), `experimental` takes the newest release in the Atom feed, prerelease
  // or not. No `channel` is passed there — electron-updater already derives the metadata filename
  // per platform *and* arch (`latest.yml`, `latest-mac.yml`, `latest-linux.yml`,
  // `latest-linux-arm64.yml`), and anything passed is used as a *prefix* to that suffix, so naming
  // the arm64 file would ask for `latest-linux-arm64-linux-arm64.yml`.
  const linuxArmChannel = process.platform === 'linux' && process.arch === 'arm64' ? 'latest-linux-arm64' : undefined;

  function applyUpdateChannel(channel: UpdateChannel) {
    if (AUTO_UPDATE_URL) {
      const base = AUTO_UPDATE_URL.endsWith('/') ? AUTO_UPDATE_URL : `${AUTO_UPDATE_URL}/`;
      autoUpdater.setFeedURL({
        provider: 'generic',
        url: `${base}${channel === 'experimental' ? 'latest/' : 'stable/'}`,
        ...(linuxArmChannel && { channel: linuxArmChannel }),
      });

      return;
    }

    autoUpdater.allowPrerelease = channel === 'experimental';
    autoUpdater.setFeedURL({
      provider: 'github',
      owner: UPDATE_REPO_OWNER,
      repo: UPDATE_REPO_NAME,
    });
  }

  ipcMain.handle('getStoreValue', (_, key: string) => {
    if (!ALLOWED_STORE_KEYS.has(key)) {
      console.warn('[store] Blocked read of unauthorized key:', key);
      return undefined;
    }
    return store.get(key);
  });

  ipcMain.handle('setStoreValue', (_, key: string, value: unknown) => {
    if (!ALLOWED_STORE_KEYS.has(key)) {
      console.warn('[store] Blocked write of unauthorized key:', key);
      return;
    }
    store.set(key, value);

    if (isSupported && key === UPDATE_CHANNEL) {
      const next = normalizeChannel(value);
      console.info(`[app-updater] Channel changed to "${next}", re-checking for updates.`);
      applyUpdateChannel(next);
      autoUpdater.checkForUpdates().catch(err => {
        console.error('[app-updater] Failed to check for updates after channel change:', err.message);
      });
    }
  });

  ipcMain.handle('app:check-for-updates', () => {
    if (!isSupported) {
      // Be honest: this build genuinely cannot self-update. Emitting a distinct signal lets the
      // renderer say so instead of faking a reassuring "you're up to date".
      sendUpdateEvent('update-unsupported');
      return;
    }
    autoUpdater.checkForUpdates().catch(err => {
      console.error('[app-updater] Failed to check for updates:', err.message);
      sendUpdateEvent('error', { message: err.message });
    });
  });

  ipcMain.on('app:quit-and-install', () => {
    autoUpdater.quitAndInstall();
  });

  if (!isSupported) return;

  applyUpdateChannel(normalizeChannel(store.get(UPDATE_CHANNEL)));

  autoUpdater.autoRunAppAfterInstall = true;
  autoUpdater.autoInstallOnAppQuit = false;

  app.whenReady().then(() => {
    const enabled = store.get(AUTO_UPDATE_ENABLED);
    if (!enabled) return;

    autoUpdater.checkForUpdates().catch(err => {
      console.error('[app-updater] Failed to check for updates:', err.message);
    });
  });

  autoUpdater.on('checking-for-update', () => {
    console.info('[app-updater] Checking for update...');
    sendUpdateEvent('checking-for-update');
  });

  autoUpdater.on('update-not-available', () => {
    console.info('[app-updater] Application is up to date.');
    sendUpdateEvent('update-not-available');
  });

  autoUpdater.on('update-available', info => {
    console.info(`[app-updater] Update available: ${info.version}`);
    sendUpdateEvent('update-available', info);
  });

  autoUpdater.on('download-progress', progressObj => {
    console.info(`[app-updater] Downloading: ${Math.round(progressObj.percent)}%`);
    sendUpdateEvent('download-progress', progressObj);
  });

  autoUpdater.on('update-cancelled', info => {
    console.info(`[app-updater] Update cancelled: ${info.version}`);
    sendUpdateEvent('update-cancelled', info);
  });

  autoUpdater.on('error', err => {
    console.error('[app-updater] Update error:', err.message);
    sendUpdateEvent('error', { message: err.message });
  });

  autoUpdater.on('update-downloaded', info => {
    console.info(`[app-updater] Update downloaded: ${info.version}`);
    sendUpdateEvent('update-downloaded', info);
  });
}
