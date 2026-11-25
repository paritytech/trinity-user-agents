import { type ConnectionMode, type ConnectionPreference } from './types';

export const CONNECTION_SETTINGS_STORAGE_KEY = 'pb:network-connection';

// Runtime source of truth for the unions — `satisfies` fails to compile if the
// two drift apart.
export const CONNECTION_MODES = ['light-client', 'rpc'] as const satisfies readonly ConnectionMode[];

export const CONNECTION_PREFERENCES = ['light-client', 'rpc', 'advanced'] as const satisfies readonly ConnectionPreference[];

// Light client was the hard-wired behaviour before this setting existed, so it
// stays the default — upgrading must not silently move anyone onto RPC nodes.
export const DEFAULT_CONNECTION_PREFERENCE: ConnectionPreference = 'light-client';

// How long a product may sit on the loading screen before we offer the RPC
// escape hatch. A cold smoldot start (warp sync from a fresh checkpoint) is the
// case this targets; a healthy light client is well under it.
export const SLOW_CONNECTION_THRESHOLD_MS = 15_000;

// Past this a load is reported as failed rather than slow. Nothing here observes
// the webview's own error events, so elapsed time is the only evidence there is —
// the state is a guess, and `did-fail-load` is what would make it a fact.
export const CONNECTION_TIMEOUT_THRESHOLD_MS = 60_000;
