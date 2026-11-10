/**
 * The single source of truth for whether auto-update can actually run. The updater (which drives
 * the checks) and the renderer settings (which show/hide the toggle, via sync IPC) must agree,
 * otherwise the UI offers a check that silently resolves to a fake "up to date".
 * Pure over its inputs so the gate is unit-testable.
 *
 * `enabledInBuild` is the explicit ENABLE_AUTO_UPDATE build-time opt-in set only by the release
 * pipeline. `hasUpdateFeed` stays a separate input because an enabled build with nothing to query —
 * neither a static feed URL nor an owner/repo — is a pipeline misconfiguration (setFeedURL would
 * build an unresolvable feed), not a valid state.
 */
export function computeAutoUpdateSupported(params: { enabledInBuild: boolean; hasUpdateFeed: boolean }): boolean {
  return params.enabledInBuild && params.hasUpdateFeed;
}

/**
 * Get OS type. Uses process.platform in Node.js (Electron main),
 * getOperatingSystem (navigator) in browser and Electron renderer.
 */
export const getOsType = (): string => {
  if (typeof process !== 'undefined' && process.platform) {
    switch (process.platform) {
      case 'darwin':
        return 'macOS';
      case 'win32':
        return 'Windows';
      case 'linux':
        return 'Linux';
      default:
        return 'Unknown';
    }
  }

  return process.release?.name ?? 'Unknown';
};
