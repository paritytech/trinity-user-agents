import * as v from 'valibot';

import { genesisHash } from '../chain/schemas';
import { type GenesisHash } from '../chain/types';

import { connectionMode, connectionPreference } from './schemas';
import {
  type AffectedChains,
  type ChainConnectionMode,
  type ConnectionMode,
  type ConnectionPreference,
  type ConnectionSettings,
} from './types';

// Under `advanced`, a network the user has never touched starts on the light
// client — the strongest option it can actually serve.
const ADVANCED_DEFAULT_MODE: ConnectionMode = 'light-client';

const isConnectionMode = (value: unknown): value is ConnectionMode => {
  return v.safeParse(connectionMode, value).success;
};

const isConnectionPreference = (value: unknown): value is ConnectionPreference => {
  return v.safeParse(connectionPreference, value).success;
};

// The mode the user asked for, before availability is taken into account.
const requestedMode = (settings: ConnectionSettings, chainId: GenesisHash): ConnectionMode => {
  if (settings.preference !== 'advanced') return settings.preference;

  return settings.overrides[chainId] ?? ADVANCED_DEFAULT_MODE;
};

// `available` is whether the embedded light client carries a chain spec for this
// network; it is passed in rather than looked up so this stays a pure helper.
// Asking for a light client anywhere else silently resolves back to RPC.
const resolveMode = (settings: ConnectionSettings, chainId: GenesisHash, available: boolean): ConnectionMode => {
  return requestedMode(settings, chainId) === 'light-client' && available ? 'light-client' : 'rpc';
};

// Whether any network could ask for a light client under these settings, so the
// boot can skip loading the light-client wasm entirely when none can. `advanced`
// counts: a network with no override falls back to `ADVANCED_DEFAULT_MODE`.
const mayUseLightClient = (settings: ConnectionSettings): boolean => settings.preference !== 'rpc';

const describeChainMode = (settings: ConnectionSettings, chainId: GenesisHash, available: boolean): ChainConnectionMode => {
  return {
    requested: requestedMode(settings, chainId),
    effective: resolveMode(settings, chainId, available),
    lightClientAvailable: available,
  };
};

// Which networks have to be moved onto a different transport for an edit to
// take effect. A preference change reaches every network at once, so it is
// reported as `all` rather than as a list this layer would have to enumerate.
// Overrides are only consulted under `advanced`, so editing one while the
// preference is elsewhere changes nothing on the wire.
const chainsAffectedBy = (previous: ConnectionSettings, next: ConnectionSettings): AffectedChains => {
  if (previous.preference !== next.preference) return 'all';
  if (next.preference !== 'advanced') return [];

  const affected: GenesisHash[] = [];
  const seen = new Set<string>();

  for (const key of [...Object.keys(previous.overrides), ...Object.keys(next.overrides)]) {
    if (seen.has(key)) continue;
    seen.add(key);

    // `Object.keys` hands back plain strings; the same parse the persisted blob
    // goes through is what recovers the branded type here. A key that fails it
    // cannot come from `setChainMode` — it is a hand-edited or corrupted blob,
    // so say so rather than dropping the network silently.
    const chainId = v.safeParse(genesisHash, key);
    if (!chainId.success) {
      console.warn('[network] ignoring an unparseable chain id in the connection overrides', key);
      continue;
    }

    if (previous.overrides[chainId.output] !== next.overrides[chainId.output]) affected.push(chainId.output);
  }

  return affected;
};

export const connectionService = {
  chainsAffectedBy,
  describeChainMode,
  isConnectionMode,
  isConnectionPreference,
  mayUseLightClient,
  resolveMode,
};
