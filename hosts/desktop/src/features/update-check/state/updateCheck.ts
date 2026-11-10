import { createState, persistLocalStorage } from '@/shared/rxstate';

// The version whose "update ready" toast the user dismissed (so we don't re-nag for it) and the
// downloaded update waiting for a relaunch. Both persist so they survive reloads.
const DISMISSED_VERSION_KEY = 'dismissed_update_version';
const PENDING_VERSION_KEY = 'pending_update_version';

// A localStorage-persisted nullable version string. `undefined` is the uninitialized sentinel
// `persistLocalStorage` never writes back; `null` is an explicit "no version". `clear()` must land
// on `null` (not the initial) so it actually persists — otherwise the stale value syncs back on the
// next launch (e.g. re-firing the "updated" toast).
function createPersistedVersion(key: string) {
  const state$ = createState<Nullable<string>>(undefined);
  persistLocalStorage(state$, { key });

  return {
    read: (): string | null => state$.get() ?? null,
    write: (version: string): void => {
      state$.set(version);
    },
    clear: (): void => {
      state$.set(null);
    },
  };
}

const pendingVersion = createPersistedVersion(PENDING_VERSION_KEY);
const dismissedVersion = createPersistedVersion(DISMISSED_VERSION_KEY);

export function getProgressPercent(data: unknown): number {
  if (data && typeof data === 'object' && 'percent' in data) {
    // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- IPC data is unknown
    const value = (data as Record<string, unknown>)['percent'];
    const num = typeof value === 'number' ? value : Number(value);
    return Number.isNaN(num) ? 0 : num;
  }
  return 0;
}

export function getErrorMessage(data: unknown, fallback: string): string {
  if (data && typeof data === 'object' && 'message' in data) {
    // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- IPC data is unknown
    const value = (data as Record<string, unknown>)['message'];
    return value != null ? String(value) : fallback;
  }
  return fallback;
}

export function getVersion(data: unknown): string | null {
  if (data && typeof data === 'object' && 'version' in data) {
    // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- IPC data is unknown
    const value = (data as Record<string, unknown>)['version'];
    return typeof value === 'string' ? value : null;
  }
  return null;
}

export const readDismissedVersion = dismissedVersion.read;
export const writeDismissedVersion = dismissedVersion.write;
export const clearDismissedVersion = dismissedVersion.clear;

export const readPendingVersion = pendingVersion.read;
export const writePendingVersion = pendingVersion.write;
export const clearPendingVersion = pendingVersion.clear;

/**
 * True when semantic version `a` is strictly newer than `b`. Compares dot-separated numeric
 * segments (1.10.0 > 1.9.0); any non-numeric/pre-release suffix on a segment is ignored, which is
 * sufficient for gating a relaunch prompt to genuinely-newer downloads. Missing segments count as 0.
 */
export function isVersionNewer(a: string, b: string): boolean {
  const segmentsA = a.split('.');
  const segmentsB = b.split('.');
  const length = Math.max(segmentsA.length, segmentsB.length);
  for (let i = 0; i < length; i++) {
    const numA = Number.parseInt(segmentsA[i] ?? '0', 10) || 0;
    const numB = Number.parseInt(segmentsB[i] ?? '0', 10) || 0;
    if (numA !== numB) return numA > numB;
  }
  return false;
}
