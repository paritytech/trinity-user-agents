import { type GenesisHash } from '../chain/types';

// What a connection actually uses on the wire.
export type ConnectionMode = 'light-client' | 'rpc';

// What the user picked on the settings page. `advanced` is not a transport — it
// hands the choice to the per-network overrides.
export type ConnectionPreference = ConnectionMode | 'advanced';

export type ConnectionSettings = {
  preference: ConnectionPreference;
  // Only consulted while the preference is `advanced`.
  overrides: Partial<Record<GenesisHash, ConnectionMode>>;
};

export type ChainConnectionMode = {
  // What the network's control shows as selected.
  requested: ConnectionMode;
  // What the connection layer will use once light-client availability is applied.
  effective: ConnectionMode;
  lightClientAvailable: boolean;
};

// The networks a settings edit has to be applied to. `all` stands in for every
// connected network, which only the connection pool knows the extent of.
export type AffectedChains = 'all' | GenesisHash[];
