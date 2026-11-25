import { SETTINGS_STORAGE_KEY } from './constants';
import { type PersistedEnvironmentScalars } from './schemas';
import { type EnvironmentId } from './types';

// Read the raw key directly: the active channel id is needed at module init,
// before the storage adapter hydrates.
const LOCAL_STORAGE_VALUE_KEY = `polkadot_${SETTINGS_STORAGE_KEY}_value`;

// Raw persisted environment id (unvalidated) or `null` when absent/unreadable.
// Validation against the known channels + the default fallback live in
// `environmentUseCase.getActiveId`.
function readPersistedEnvironmentId(): string | null {
  try {
    const raw = localStorage.getItem(LOCAL_STORAGE_VALUE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw);
      if (parsed && typeof parsed === 'object' && typeof parsed.environmentId === 'string') {
        return parsed.environmentId;
      }
    }
  } catch (e) {
    console.error('[environment] failed to read settings', e);
  }
  return null;
}

// Keyed per channel: `alpha` and `beta` serve different backends, so one must
// never restore the other's URLs.
function lastKnownScalarsKey(id: EnvironmentId): string {
  return `pb:last-known-env:${id}`;
}

function persistLastKnownScalars(id: EnvironmentId, value: PersistedEnvironmentScalars): void {
  try {
    localStorage.setItem(lastKnownScalarsKey(id), JSON.stringify(value));
  } catch (error) {
    console.warn(`[environment] could not persist the last-known scalars for "${id}"`, error);
  }
}

// Unvalidated — the caller parses it through `persistedEnvironmentScalarsSchema`.
function readLastKnownScalars(id: EnvironmentId): unknown {
  try {
    const raw = localStorage.getItem(lastKnownScalarsKey(id));

    return raw ? JSON.parse(raw) : null;
  } catch (error) {
    console.warn(`[environment] could not read the last-known scalars for "${id}"`, error);

    return null;
  }
}

export const environmentRepository = {
  readPersistedEnvironmentId,
  persistLastKnownScalars,
  readLastKnownScalars,
};
