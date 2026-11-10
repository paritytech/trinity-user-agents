import { ipcMain } from 'electron';

import { computeAutoUpdateSupported } from './lib/utils';

/**
 * The single runtime gate for auto-update, gathering the environment inputs and delegating the
 * decision to the pure `computeAutoUpdateSupported`. Every consumer (the updater, and the renderer
 * via sync IPC) reads this so they can never disagree — the bug where a "Check for updates" action
 * silently resolved to a fake "up to date" came from two predicates diverging.
 *
 * All four env vars are inlined at build time by the `define` block in vite.config.main.ts:
 * ENABLE_AUTO_UPDATE is the release pipeline's explicit opt-in; without it (local, e2e and PR
 * builds) the app honestly reports that it cannot self-update. The rest name the feed — either
 * AUTO_UPDATE_URL for a static one or UPDATE_REPO_OWNER/NAME for GitHub Releases — and the latter
 * pair carries a default, so in practice only the opt-in varies between builds.
 */
export function getAutoUpdateSupported(): boolean {
  const hasStaticFeed = (process.env['AUTO_UPDATE_URL'] ?? '').trim().length > 0;
  const hasUpdateRepo =
    (process.env['UPDATE_REPO_OWNER'] ?? '').trim().length > 0 && (process.env['UPDATE_REPO_NAME'] ?? '').trim().length > 0;

  return computeAutoUpdateSupported({
    enabledInBuild: (process.env['ENABLE_AUTO_UPDATE'] ?? '').trim() === 'true',
    hasUpdateFeed: hasStaticFeed || hasUpdateRepo,
  });
}

export function registerAutoUpdateSupportedSyncIpc(): void {
  ipcMain.on('sync:isAutoUpdateSupported', event => {
    event.returnValue = getAutoUpdateSupported();
  });
}
