import * as v from 'valibot';

import { genesisHash } from '../chain/schemas';

import { CONNECTION_MODES, CONNECTION_PREFERENCES, DEFAULT_CONNECTION_PREFERENCE } from './constants';
import { type ConnectionSettings } from './types';

export const connectionMode = v.picklist(CONNECTION_MODES);

export const connectionPreference = v.picklist(CONNECTION_PREFERENCES);

// Overrides are recovered entry by entry instead of validated as a whole: an
// older build or a hand edit can leave one bad key in the blob, and rejecting
// the record wholesale would silently drop every other network's choice with it.
const connectionOverrides = v.pipe(
  v.fallback(v.record(v.string(), v.unknown()), {}),
  v.transform((raw): ConnectionSettings['overrides'] => {
    const overrides: ConnectionSettings['overrides'] = {};

    for (const [key, value] of Object.entries(raw)) {
      const chainId = v.safeParse(genesisHash, key);
      const mode = v.safeParse(connectionMode, value);

      if (chainId.success && mode.success) {
        overrides[chainId.output] = mode.output;
      }
    }

    return overrides;
  }),
);

// Every field falls back rather than failing the parse — the app must always end
// up with a usable mode, even reading a blob it does not recognise.
export const connectionSettingsSchema = v.fallback(
  v.object({
    preference: v.fallback(connectionPreference, DEFAULT_CONNECTION_PREFERENCE),
    overrides: connectionOverrides,
  }),
  { preference: DEFAULT_CONNECTION_PREFERENCE, overrides: {} },
);
