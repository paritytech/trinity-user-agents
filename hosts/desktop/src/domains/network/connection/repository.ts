import * as v from 'valibot';

import { createState, persistLocalStorage } from '@/shared/rxstate';
import { type GenesisHash } from '../chain/types';

import { CONNECTION_SETTINGS_STORAGE_KEY } from './constants';
import { connectionSettingsSchema } from './schemas';
import { type ConnectionMode, type ConnectionPreference, type ConnectionSettings } from './types';

// Read the raw key directly rather than through `persistLocalStorage`'s async
// hydration: the connection layer resolves a mode synchronously, the first time
// a chain is locked, which can land before an async read would have resolved.
const LOCAL_STORAGE_VALUE_KEY = `polkadot_${CONNECTION_SETTINGS_STORAGE_KEY}_value`;

function readPersisted(): ConnectionSettings {
  try {
    const raw = localStorage.getItem(LOCAL_STORAGE_VALUE_KEY);

    return v.parse(connectionSettingsSchema, raw ? JSON.parse(raw) : null);
  } catch (error) {
    console.error('[network] failed to read connection settings', error);

    return v.parse(connectionSettingsSchema, null);
  }
}

// Taken once at module load, for the synchronous first read described above.
// Later edits reach the wire without it: `initConnectionModeSwitch` has the pool
// rebuild the affected transports, and `createProvider` re-reads `getSettings()`.
const initialSettings = readPersisted();

const settings$ = createState<ConnectionSettings>(initialSettings);

persistLocalStorage(settings$, { key: CONNECTION_SETTINGS_STORAGE_KEY, sync: false });

export const connectionRepository = {
  settings$,
  initialSettings,
  getSettings(): ConnectionSettings {
    return settings$.get();
  },
  setPreference(preference: ConnectionPreference) {
    return settings$.set(prev => ({ ...prev, preference }));
  },
  // `null` drops the override, putting the network back on the advanced default.
  setChainMode(chainId: GenesisHash, mode: ConnectionMode | null) {
    return settings$.set(prev => {
      const overrides = { ...prev.overrides };

      if (mode === null) {
        delete overrides[chainId];
      } else {
        overrides[chainId] = mode;
      }

      return { ...prev, overrides };
    });
  },
};
